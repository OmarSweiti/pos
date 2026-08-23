"""Small fail-closed Rust lexer shared by repository policy checks.

This is deliberately not a Rust parser. It separates identifiers, punctuation,
strings, characters, and comments accurately enough that policy text hidden in
comments or literals cannot impersonate executable source. Block comments nest
and raw strings may use any number of ``#`` delimiters, matching Rust's lexical
rules for the constructs these checks inspect.
"""

from __future__ import annotations

from dataclasses import dataclass


class RustLexError(ValueError):
    """Source could not be tokenized without ambiguity."""


@dataclass(frozen=True)
class RustToken:
    kind: str
    value: str
    offset: int
    # Raw identifiers are semantically the identifier after `r#`, but a
    # consumer that distinguishes language keywords from identifiers can still
    # reject a raw spelling in that position.
    raw: bool = False


def rust_tokens(source: str) -> list[RustToken]:
    tokens: list[RustToken] = []
    index = 0
    size = len(source)

    while index < size:
        char = source[index]
        if char.isspace():
            index += 1
            continue

        if source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = size if newline < 0 else newline + 1
            continue

        if source.startswith("/*", index):
            start = index
            depth = 1
            index += 2
            while index < size and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            if depth:
                raise RustLexError(f"unterminated Rust block comment at byte {start}")
            continue

        # Raw identifiers such as r#const are identifiers, not keywords.
        if source.startswith("r#", index) and index + 2 < size and (
            source[index + 2].isalpha() or source[index + 2] == "_"
        ):
            start = index
            index += 3
            while index < size and (
                source[index].isalnum() or source[index] == "_"
            ):
                index += 1
            tokens.append(RustToken("ident", source[start + 2 : index], start, True))
            continue

        # r"...", r#"..."#, br#"..."#, and cr#"..."#.
        raw_prefix = None
        for prefix in ("br", "cr", "r"):
            if not source.startswith(prefix, index):
                continue
            marker = index + len(prefix)
            hashes = 0
            while marker + hashes < size and source[marker + hashes] == "#":
                hashes += 1
            quote = marker + hashes
            if quote < size and source[quote] == '"':
                raw_prefix = (hashes, quote)
                break
        if raw_prefix is not None:
            hashes, quote = raw_prefix
            start = index
            content_start = quote + 1
            terminator = '"' + ("#" * hashes)
            end = source.find(terminator, content_start)
            if end < 0:
                raise RustLexError(f"unterminated Rust raw string at byte {start}")
            tokens.append(RustToken("string", source[content_start:end], start))
            index = end + len(terminator)
            continue

        # Ordinary and byte/C strings. Prefixes are separate identifiers, which
        # is harmless because consumers ignore string contents.
        if char == '"':
            start = index
            index += 1
            content_start = index
            escaped = False
            while index < size:
                if source[index] == '"' and not escaped:
                    break
                if source[index] == "\\" and not escaped:
                    escaped = True
                else:
                    escaped = False
                index += 1
            if index >= size:
                raise RustLexError(f"unterminated Rust string at byte {start}")
            tokens.append(RustToken("string", source[content_start:index], start))
            index += 1
            continue

        # A short quoted token is a character; a lifetime has no nearby quote.
        if char == "'":
            start = index
            cursor = index + 1
            if cursor < size and source[cursor] == "\\":
                cursor += 2
                while (
                    cursor < size
                    and source[cursor] != "'"
                    and cursor - start < 12
                ):
                    cursor += 1
            else:
                cursor += 1
            if cursor < size and source[cursor] == "'":
                tokens.append(RustToken("char", source[index + 1 : cursor], start))
                index = cursor + 1
                continue

        if char.isalpha() or char == "_":
            start = index
            index += 1
            while index < size and (
                source[index].isalnum() or source[index] == "_"
            ):
                index += 1
            tokens.append(RustToken("ident", source[start:index], start))
            continue

        tokens.append(RustToken("punct", char, index))
        index += 1

    return tokens
