#!/usr/bin/env python3
"""Enforce the pos-domain module dependency rule (ref/domain-api.md §15).

    money ─┬─→ tax ──┬─→ cart ──┬─→ tender ──→ receipt
           │         │          ├─→ refund
           ├─→ stock │          └─→ promo
           └─→ ids ──┴─→ time
                         permissions ──→ audit

Arrows point one way. A cycle here is a design error.

Why not `cargo modules dependencies --acyclic`: its graph is item-level, so
`Money::from_minor -> Money` (any constructor returning Self) is reported as a
circular dependency. It cannot express "modules must be acyclic".
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

SRC = Path(__file__).resolve().parent.parent / "crates" / "pos-domain" / "src"

# `use crate::x`, `use super::x`, `use self::x`, and `use crate::{a, b}`
USE_RE = re.compile(r"^\s*(?:pub\s+)?use\s+(crate|super|self)::([A-Za-z0-9_{}, :]+)")
HEAD_RE = re.compile(r"([a-z_][a-z0-9_]*)")


def modules() -> list[str]:
    return sorted(
        p.stem for p in SRC.glob("*.rs") if p.stem not in {"lib", "main"}
    )


def strip_comments(text: str) -> str:
    text = re.sub(r"//.*", "", text)
    return re.sub(r"/\*.*?\*/", "", text, flags=re.S)


def edges(mods: set[str]) -> dict[str, set[str]]:
    graph: dict[str, set[str]] = {m: set() for m in mods}
    for path in SRC.glob("*.rs"):
        me = path.stem
        if me not in graph:
            continue
        for line in strip_comments(path.read_text(encoding="utf-8")).splitlines():
            m = USE_RE.match(line)
            if not m:
                continue
            root, rest = m.group(1), m.group(2)
            # `use super::*` / `use self::*` inside an inline mod refer to the
            # same file; only `crate::` crosses a module boundary.
            if root != "crate":
                continue
            for name in HEAD_RE.findall(rest.split("::")[0]):
                if name in graph and name != me:
                    graph[me].add(name)
    return graph


def find_cycle(graph: dict[str, set[str]]) -> list[str] | None:
    WHITE, GREY, BLACK = 0, 1, 2
    color = dict.fromkeys(graph, WHITE)
    stack: list[str] = []

    def visit(node: str) -> list[str] | None:
        color[node] = GREY
        stack.append(node)
        for nxt in sorted(graph[node]):
            if color[nxt] == GREY:
                return stack[stack.index(nxt):] + [nxt]
            if color[nxt] == WHITE:
                found = visit(nxt)
                if found:
                    return found
        stack.pop()
        color[node] = BLACK
        return None

    for node in sorted(graph):
        if color[node] == WHITE:
            cycle = visit(node)
            if cycle:
                return cycle
    return None


def main() -> int:
    if not SRC.is_dir():
        print(f"pos-domain source not found at {SRC}", file=sys.stderr)
        return 1

    mods = modules()
    if not mods:
        print("no pos-domain modules found — nothing to check")
        return 0

    graph = edges(set(mods))
    cycle = find_cycle(graph)
    if cycle:
        print("pos-domain module cycle detected — dependency arrows point one way")
        print("  " + " -> ".join(cycle))
        print("\nSee docs/implementation/ref/domain-api.md §15.")
        return 1

    total = sum(len(v) for v in graph.values())
    print(f"pos-domain module graph acyclic ({len(mods)} modules, {total} edges)")
    for m in mods:
        if graph[m]:
            print(f"  {m} -> {', '.join(sorted(graph[m]))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
