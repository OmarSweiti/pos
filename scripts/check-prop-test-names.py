#!/usr/bin/env python3
"""Property tests are named `prop_<invariant>`; example tests are not.

`ref/domain-api.md` is normative for pos-domain, and it names every property test
with a `prop_` prefix. Microstep 1.1.5 verifies the money suite with

    cargo nextest run -p pos-domain money::tests::prop_

which is a *filter*. A property test that loses the prefix is omitted while the
other matching properties can still pass. If no tests match at all, current
nextest correctly exits nonzero; the dangerous case is a partial suite that stays
green while silently leaving the renamed property out.

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

import sys
from bisect import bisect_right
from pathlib import Path

from rust_lexer import RustLexError, RustToken, rust_tokens

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"

OPEN_TO_CLOSE = {"{": "}", "[": "]", "(": ")"}
CLOSE_TO_OPEN = {close: open_ for open_, close in OPEN_TO_CLOSE.items()}


def is_punct(token: RustToken, value: str) -> bool:
    return token.kind == "punct" and token.value == value


def identifier_name(token: RustToken) -> str | None:
    """Return an identifier's semantic spelling, including raw identifiers."""
    if token.kind != "ident":
        return None
    return token.value[2:] if token.value.startswith("r#") else token.value


def line_number(newlines: list[int], offset: int) -> int:
    return bisect_right(newlines, offset) + 1


def matching_delimiters(tokens: list[RustToken]) -> dict[int, int]:
    """Return opening-token -> closing-token indexes, rejecting ambiguity."""
    stack: list[tuple[str, int]] = []
    matches: dict[int, int] = {}

    for index, token in enumerate(tokens):
        if token.kind != "punct":
            continue
        if token.value in OPEN_TO_CLOSE:
            stack.append((token.value, index))
            continue
        if token.value not in CLOSE_TO_OPEN:
            continue
        if not stack or stack[-1][0] != CLOSE_TO_OPEN[token.value]:
            raise RustLexError(
                f"unmatched Rust delimiter `{token.value}` at character {token.offset}"
            )
        _, opening_index = stack.pop()
        matches[opening_index] = index

    if stack:
        opening, opening_index = stack[-1]
        raise RustLexError(
            f"unclosed Rust delimiter `{opening}` at character "
            f"{tokens[opening_index].offset}"
        )
    return matches


def curly_contexts(tokens: list[RustToken]) -> list[tuple[int, ...]]:
    """Record the containing brace scopes for each token."""
    contexts: list[tuple[int, ...]] = []
    stack: list[int] = []
    for index, token in enumerate(tokens):
        contexts.append(tuple(stack))
        if is_punct(token, "{"):
            stack.append(index)
        elif is_punct(token, "}"):
            stack.pop()
    return contexts


def proptest_aliases(
    tokens: list[RustToken], contexts: list[tuple[int, ...]]
) -> dict[tuple[int, ...], set[str]]:
    """Collect `use ... proptest as alias` bindings by lexical brace scope."""
    aliases: dict[tuple[int, ...], set[str]] = {}
    index = 0
    while index < len(tokens):
        # Raw `r#use` is an identifier, and `$use` may be a macro metavariable;
        # neither begins an import item.
        if (
            tokens[index].kind != "ident"
            or tokens[index].value != "use"
            or (index > 0 and is_punct(tokens[index - 1], "$"))
        ):
            index += 1
            continue

        end = index + 1
        while end < len(tokens) and not is_punct(tokens[end], ";"):
            end += 1

        for target in range(index + 1, max(index + 1, end - 1)):
            if identifier_name(tokens[target]) != "proptest":
                continue
            if (
                target + 2 >= end
                or tokens[target + 1].kind != "ident"
                or tokens[target + 1].value != "as"
            ):
                continue
            alias = identifier_name(tokens[target + 2])
            if alias and alias != "_":
                aliases.setdefault(contexts[index], set()).add(alias)

        index = end + 1
    return aliases


def alias_is_visible(
    name: str,
    context: tuple[int, ...],
    aliases: dict[tuple[int, ...], set[str]],
) -> bool:
    return any(
        name in names and context[: len(scope)] == scope
        for scope, names in aliases.items()
    )


def proptest_ranges(
    tokens: list[RustToken],
    matches: dict[int, int],
    contexts: list[tuple[int, ...]],
) -> list[tuple[int, int]]:
    """Locate token ranges belonging to real `proptest!` invocations."""
    aliases = proptest_aliases(tokens, contexts)
    ranges: list[tuple[int, int]] = []
    for index in range(len(tokens) - 2):
        name = identifier_name(tokens[index])
        if name is None or (
            name != "proptest"
            and not alias_is_visible(name, contexts[index], aliases)
        ):
            continue
        if not is_punct(tokens[index + 1], "!"):
            continue
        opening = tokens[index + 2]
        if opening.kind != "punct" or opening.value not in OPEN_TO_CLOSE:
            continue
        ranges.append((index + 2, matches[index + 2]))
    return ranges


def is_inside(index: int, ranges: list[tuple[int, int]]) -> bool:
    return any(opening <= index <= closing for opening, closing in ranges)


def scan(text: str) -> tuple[list[tuple[int, str]], list[tuple[int, str]]]:
    """(unprefixed inside a proptest! block, prop_-prefixed example tests)."""
    inside: list[tuple[int, str]] = []
    outside: list[tuple[int, str]] = []
    tokens = rust_tokens(text)
    matches = matching_delimiters(tokens)
    contexts = curly_contexts(tokens)
    ranges = proptest_ranges(tokens, matches, contexts)
    newlines = [index for index, char in enumerate(text) if char == "\n"]

    # Inspect every function declaration token inside a proptest invocation. The
    # lexer has already removed comments and isolated literal contents, so a
    # brace or `fn` embedded in either cannot alter this classification.
    for index, token in enumerate(tokens[:-1]):
        if token.kind != "ident" or token.value != "fn":
            continue
        name = tokens[index + 1]
        if name.kind != "ident" or not is_inside(index, ranges):
            continue
        semantic_name = identifier_name(name)
        if semantic_name is not None and not semantic_name.startswith("prop_"):
            inside.append((line_number(newlines, name.offset), name.value))

    # Outside a proptest invocation, pair an exact #[test] attribute with the
    # next function declaration. Reset at each proptest boundary so an attribute
    # cannot leak into or out of a macro invocation.
    saw_test_attr = False
    was_inside = False
    for index, token in enumerate(tokens):
        now_inside = is_inside(index, ranges)
        if now_inside != was_inside:
            saw_test_attr = False
            was_inside = now_inside
        if now_inside:
            continue

        if (
            is_punct(token, "#")
            and index + 3 < len(tokens)
            and is_punct(tokens[index + 1], "[")
            and identifier_name(tokens[index + 2]) == "test"
            and is_punct(tokens[index + 3], "]")
        ):
            saw_test_attr = True
            continue
        if token.kind == "ident" and token.value == "fn" and index + 1 < len(tokens):
            name = tokens[index + 1]
            if (
                saw_test_attr
                and identifier_name(name) is not None
                and identifier_name(name).startswith("prop_")
            ):
                outside.append((line_number(newlines, name.offset), name.value))
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
            try:
                text = path.read_text(encoding="utf-8")
                inside, outside = scan(text)
            except (OSError, UnicodeError, RustLexError) as error:
                problems.append(
                    f"{rel(path)}  could not safely scan Rust source: {error}"
                )
                continue
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

    cases: list[tuple[str, str, str | None]] = [
        ("a prefixed property test passes", """
proptest! {
    #[test]
    fn prop_split_preserves_total(x in 0i64..9) { }
}
""", None),
        ("an unprefixed property test fails", """
proptest! {
    #[test]
    fn split_preserves_total(x in 0i64..9) { }
}
""", "`split_preserves_total`"),
        ("the second one in a block is caught too", """
proptest! {
    #[test]
    fn prop_ok(x in 0i64..9) { }
    #[test]
    fn add_sub_roundtrip(a in 0i64..9) { }
}
""", "`add_sub_roundtrip`"),
        ("an example test needs no prefix", """
#[test]
fn jod_exponent_is_three() { }
""", None),
        ("an example test may not borrow the prefix", """
#[test]
fn prop_looks_like_a_property_test() { }
""", "`prop_looks_like_a_property_test`"),
        ("a plain fn after the block is not a test", """
proptest! {
    #[test]
    fn prop_ok(x in 0i64..9) { }
}

fn helper() -> i64 { 0 }
""", None),
        ("nested braces do not end the block early", """
proptest! {
    #[test]
    fn prop_ok(x in 0i64..9) {
        if x > 0 { let _ = x; }
    }
    #[test]
    fn missing_prefix(y in 0i64..9) { }
}
""", "`missing_prefix`"),
        ("a brace in a string cannot end the block early", r'''
proptest! {
    #[test]
    fn prop_string_brace(x in 0i64..9) { let _ = "}"; }
    #[test]
    fn missing_after_string(y in 0i64..9) { }
}
''', "`missing_after_string`"),
        ("a brace in a line comment cannot end the block early", """
proptest! {
    #[test]
    fn prop_comment_brace(x in 0i64..9) {
        // }
        let _ = x;
    }
    #[test]
    fn missing_after_comment(y in 0i64..9) { }
}
""", "`missing_after_comment`"),
        ("nested block-comment braces cannot end the block early", """
proptest! {
    #[test]
    fn prop_block_comment(x in 0i64..9) {
        /* outer } /* inner } */ still outer } */
        let _ = x;
    }
    #[test]
    fn missing_after_block_comment(y in 0i64..9) { }
}
""", "`missing_after_block_comment`"),
        ("a fake macro in a comment or string is ignored", r'''
// proptest! {
const NOTE: &str = "proptest! { fn hidden() {} }";
#[test]
fn ordinary_example() { }
''', None),
        ("parenthesized proptest invocations are covered", """
proptest! (
    #[test]
    fn missing_in_parentheses(x in 0i64..9) { }
);
""", "`missing_in_parentheses`"),
        ("an imported proptest macro alias is covered", """
use proptest::proptest as property;

property! {
    #[test]
    fn missing_through_alias(x in 0i64..9) { }
}
""", "`missing_through_alias`"),
        ("a grouped prelude alias with raw identifiers is covered", """
use proptest::prelude::{r#proptest as r#property};

r#property! {
    #[test]
    fn missing_through_raw_alias(x in 0i64..9) { }
}
""", "`missing_through_raw_alias`"),
        ("a raw property function name is normalized", """
r#proptest! {
    #[test]
    fn r#prop_raw_name(x in 0i64..9) { }
}
""", None),
        ("an alias does not leak into a sibling scope", """
mod imports_property {
    use proptest::proptest as property;
}

mod unrelated_macro {
    macro_rules! property {
        ($($tokens:tt)*) => {};
    }
    property! {
        fn not_a_property_test() { }
    }
}
""", None),
        ("an unterminated string fails closed", '''
const BROKEN: &str = "unterminated;
''', "could not safely scan Rust source"),
        ("an unterminated block comment fails closed", """
/* never closed
""", "could not safely scan Rust source"),
        ("an unclosed delimiter fails closed", """
fn broken() {
""", "could not safely scan Rust source"),
    ]

    passed = failed = 0
    for label, body, wanted_fragment in cases:
        with tempfile.TemporaryDirectory() as tmp:
            crate = Path(tmp) / "src"
            crate.mkdir(parents=True)
            (crate / "lib.rs").write_text(body)
            problems = check([Path(tmp)])
        matched = (
            not problems
            if wanted_fragment is None
            else any(wanted_fragment in problem for problem in problems)
        )
        if matched:
            print(f"  ok    {label}")
            passed += 1
        else:
            print(
                f"  FAIL  {label} "
                f"(wanted fragment={wanted_fragment!r}, got {problems!r})"
            )
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
            "microstep 1.1.5 verifies them with `money::tests::prop_`; a property "
            "whose name drifts is omitted from that otherwise-green filtered run."
        )
        return 1
    print("property tests are named prop_<invariant> (ref/domain-api.md)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
