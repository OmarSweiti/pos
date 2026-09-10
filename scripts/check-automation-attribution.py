#!/usr/bin/env python3
"""Reject assistant attribution with one narrow Dependabot compatibility case.

An exact Dependabot author name/email may retain its exact trailer because that
combination already exists in repository history. Git author metadata is locally
configurable, so this is not cryptographic proof of GitHub App identity. Coding
assistants are tools, not co-authors: do not add a machine Co-Authored-By trailer
or a Generated-with/by line. A human remains accountable for review.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

MACHINE_TRAILER = re.compile(
    r"^[ \t]*co-authored-by:.*(?:"
    r"\[bot\]|anthropic\.com|openai\.com|copilot@|cursor\.com|cognition\.ai|"
    r"codeium|windsurf|tabnine|sourcegraph\.com|replit\.com|devin@|aider@"
    r")",
    re.IGNORECASE | re.MULTILINE,
)
DEPENDABOT_NAME = "dependabot[bot]"
DEPENDABOT_EMAIL = "49699333+dependabot[bot]@users.noreply.github.com"
DEPENDABOT_TRAILER = re.compile(
    r"^[ \t]*co-authored-by:[ \t]*dependabot\[bot\][ \t]*"
    r"<49699333\+dependabot\[bot\]@users\.noreply\.github\.com>[ \t]*$",
    re.IGNORECASE | re.MULTILINE,
)
ASSISTANT_IDENTITY = re.compile(
    r"^[ \t]*co-authored-by:[ \t]*(?:"
    r"(?:(?:openai|gpt-[0-9.]+)[ \t]+)?codex|chatgpt|(?:github[ \t]+)?copilot|"
    r"anthropic[ \t]+claude|"
    r"claude[ \t]+(?:(?:opus|sonnet|haiku|code)(?:[ \t]+[0-9][^<]*)?|[0-9][^<]*)|"
    r"cursor(?:[ \t]+ai)?|devin[ \t]+ai|aider|codeium|windsurf|tabnine|"
    r"sourcegraph[ \t]+cody|replit[ \t]+agent|amazon[ \t]+q(?:[ \t]+developer)?|"
    r"gemini[ \t]+code[ \t]+assist|cline|roo[ \t]+code"
    r")[ \t]*<",
    re.IGNORECASE | re.MULTILINE,
)
GENERATED_ASSISTANT = (
    r"claude|anthropic|openai|chatgpt|gpt[- .]?[0-9]|codex|copilot|cursor|"
    r"devin|aider|codeium|windsurf|tabnine|sourcegraph|cody|replit|"
    r"amazon[ \t]+q|gemini|cline|roo[ \t]+code"
)
GENERATED_LINE = re.compile(
    r"^[ \t]*(?:🤖[ \t]*)?generated(?:[ \t]+|-)(?:with|by)[ \t]*:?[ \t]+.*(?:"
    + GENERATED_ASSISTANT
    + r")",
    re.IGNORECASE | re.MULTILINE,
)


def has_forbidden_attribution(message: str, *, matching_dependabot_author: bool = False) -> bool:
    # GitHub's Dependabot squash commits add this exact trailer even though the
    # same bot identity is already the author. Preserve that existing shape only
    # when the name/email strings match exactly. Those strings are spoofable Git
    # metadata, so this is compatibility—not authentication. A message-file check
    # cannot claim the exception, and it never exempts another assistant line.
    if matching_dependabot_author:
        message = DEPENDABOT_TRAILER.sub("", message)
    return any(
        pattern.search(message)
        for pattern in (MACHINE_TRAILER, ASSISTANT_IDENTITY, GENERATED_LINE)
    )


def read_commit(commit: str) -> tuple[str, str, str]:
    completed = subprocess.run(
        ["git", "show", "--no-patch", "--format=%an%x00%ae%x00%B", commit],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", "replace").strip()
        raise RuntimeError(f"git could not read commit {commit}: {detail}")
    fields = completed.stdout.decode("utf-8", "replace").split("\0", 2)
    if len(fields) != 3:
        raise RuntimeError(f"git returned malformed author metadata for commit {commit}")
    return fields[0], fields[1], fields[2]


def refusal_advice() -> str:
    return (
        "  Remove the machine Co-Authored-By/Generated-with line. The exact Dependabot\n"
        "  author/trailer combination is the only compatibility exception."
    )


def read_commits(commits_file: Path) -> list[tuple[str, str, str, str]]:
    """Every commit in one `git log`, as (sha, author name, author email, message).

    `.githooks/pre-push` spawned this program once per pushed commit, and
    `scripts/run-python.sh` starts an interpreter twice per spawn (a version
    probe, then exec). That is ~150 ms of process start-up per commit and it is
    all serial: measured 1.36 s for a ten-commit push and 19.16 s for 138.

    `--no-walk` keeps `git log` from following parents, so this reads exactly the
    commits it is given and nothing else — the caller has already decided the set.
    """
    payload = commits_file.read_bytes()
    if not payload.strip():
        return []
    wanted = len([line for line in payload.split(b"\n") if line.strip()])

    completed = subprocess.run(
        ["git", "log", "--no-walk", "--stdin", "--format=%H%x00%an%x00%ae%x00%B%x00%x00"],
        check=False,
        input=payload,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", "replace").strip()
        raise RuntimeError(f"git could not read the pushed commits: {detail}")

    records: list[tuple[str, str, str, str]] = []
    for chunk in completed.stdout.decode("utf-8", "replace").split("\0\0"):
        # git log writes a newline between entries, so every chunk after the
        # first opens with one, and the stream ends with a bare newline.
        chunk = chunk.lstrip("\n")
        if not chunk:
            continue
        fields = chunk.split("\0", 3)
        if len(fields) != 4:
            raise RuntimeError("git returned malformed author metadata in the batch")
        records.append((fields[0], fields[1], fields[2], fields[3]))

    # Fail closed on a short read. A batch that silently returned fewer commits
    # than it was handed would wave the rest through, which is the one way this
    # optimisation could become a hole rather than a speed-up.
    if len(records) != wanted:
        raise RuntimeError(
            f"git described {len(records)} of {wanted} pushed commits; refusing to guess"
        )
    return records


def self_test() -> int:
    cases = (
        (True, "Claude trailer", "Co-Authored-By: Claude <noreply@anthropic.com>"),
        (True, "Codex generated-by", "Generated by OpenAI Codex"),
        (True, "ChatGPT generated-by", "Generated by ChatGPT"),
        (
            True,
            "promotion title Generated by ChatGPT",
            "Generated by ChatGPT\nPromotion evidence follows.",
        ),
        (True, "ChatGPT canonical generated-by trailer", "Generated-by: ChatGPT"),
        (True, "Gemini generated-with", "Generated with Gemini Code Assist"),
        (True, "Claude canonical generated-with trailer", "Generated-with: Claude Code"),
        (True, "Amazon Q generated-by", "Generated by Amazon Q Developer"),
        (True, "Cline generated-with", "Generated with Cline"),
        (True, "bot trailer", "Co-authored-by: helper[bot] <1+helper[bot]@users.noreply.github.com>"),
        (True, "model through human-looking address", "Co-authored-by: Claude Opus 5 <person@example.com>"),
        (True, "Codex through human-looking address", "Co-authored-by: OpenAI Codex <person@example.com>"),
        (True, "Copilot through human-looking address", "Co-authored-by: GitHub Copilot <person@example.com>"),
        (True, "ChatGPT through human-looking address", "Co-authored-by: ChatGPT <person@example.com>"),
        (False, "human co-author", "Co-Authored-By: Claude Dubois <claude.dubois@example.fr>"),
        (False, "human named Cody", "Co-Authored-By: Cody Fisher <cody@example.com>"),
        (False, "bot authorship belongs in metadata, not a trailer", "chore(repo): bump serde   [—]"),
        (False, "commented example", "# Co-Authored-By: Claude <noreply@anthropic.com>"),
        (False, "ordinary generated prose", "Generated receipts are checked by the printer simulator."),
    )
    failed = 0
    for expected, label, message in cases:
        actual = has_forbidden_attribution(message)
        if actual == expected:
            print(f"  ok      {label}")
        else:
            print(f"  FAILED  {label}")
            failed += 1
    dependabot_trailer = (
        "Co-authored-by: dependabot[bot] "
        "<49699333+dependabot[bot]@users.noreply.github.com>"
    )
    if not has_forbidden_attribution(
        dependabot_trailer, matching_dependabot_author=True
    ):
        print("  ok      exact Dependabot trailer with matching author metadata")
    else:
        print("  FAILED  exact Dependabot trailer with matching author metadata")
        failed += 1
    if has_forbidden_attribution(
        dependabot_trailer + "\nGenerated by OpenAI Codex",
        matching_dependabot_author=True,
    ):
        print("  ok      Dependabot cannot exempt another assistant attribution")
    else:
        print("  FAILED  Dependabot cannot exempt another assistant attribution")
        failed += 1
    total = len(cases) + 2
    print(f"\n{total - failed} passed, {failed} failed")
    return 1 if failed else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--message-file", type=Path)
    source.add_argument("--git-commit")
    source.add_argument("--commits-file", type=Path)
    source.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if args.commits_file is not None:
        try:
            records = read_commits(args.commits_file)
        except (OSError, RuntimeError) as exc:
            print(f"attribution-policy: ERROR — {exc}", file=sys.stderr)
            return 2
        refused = [
            sha
            for sha, name, email, message in records
            if has_forbidden_attribution(
                message,
                matching_dependabot_author=(
                    name == DEPENDABOT_NAME and email == DEPENDABOT_EMAIL
                ),
            )
        ]
        if not refused:
            return 0
        # Name every offending commit, not just the first: a rebase that has to
        # rewrite four of them is one operation, and finding them one push at a
        # time is four round trips.
        for sha in refused:
            print(f"attribution-policy: REFUSED — {sha} carries assistant attribution.", file=sys.stderr)
        print(refusal_advice(), file=sys.stderr)
        return 1

    try:
        matching_dependabot_author = False
        if args.message_file is not None:
            message = args.message_file.read_text(encoding="utf-8", errors="replace")
        else:
            author_name, author_email, message = read_commit(args.git_commit)
            matching_dependabot_author = (
                author_name == DEPENDABOT_NAME and author_email == DEPENDABOT_EMAIL
            )
    except (OSError, RuntimeError) as exc:
        print(f"attribution-policy: ERROR — {exc}", file=sys.stderr)
        return 2

    if not has_forbidden_attribution(
        message, matching_dependabot_author=matching_dependabot_author
    ):
        return 0

    print(
        "attribution-policy: REFUSED — coding assistants are tools, not commit co-authors.",
        file=sys.stderr,
    )
    print(refusal_advice(), file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
