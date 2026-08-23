#!/usr/bin/env python3
"""Codex PreToolUse adapter for the repository's immutable-path policy.

Codex reports shell commands as ``Bash`` and file edits as one ``apply_patch``
call containing a patch string. The established implementation in
``.claude/hooks/protect-immutable.py`` already owns the path policy and shell
parser; this adapter translates Codex's payloads into those shared functions.

Exit code 2 blocks the tool call. Internal failures deliberately fail open,
matching the shared guard; git hooks and CI remain the non-agent backstops.
"""

from __future__ import annotations

import json
import os
import re
import runpy
import shlex
import sys
from pathlib import Path

PATCH_PATH = re.compile(
    r"^\*\*\* (?:(?:Add|Update|Delete) File:|Move to:)\s*(.+?)\s*$"
)
RELEVANT = ("migrations", ".sql", "docs/plan", "plan", "sqlx")
SHELL_SEGMENT = re.compile(r"\n|;|&&|\|\||\||\$\(|`")
WINDOWS_SEPARATOR = re.compile(r"\\(?=[A-Za-z0-9_.])")
TRANSPARENT_WRAPPERS = frozenset(
    {"command", "env", "exec", "nohup", "sudo", "time", "xargs"}
)
SHELL_INTERPRETERS = frozenset(
    {
        "bash",
        "cmd",
        "cmd.exe",
        "dash",
        "ksh",
        "powershell",
        "powershell.exe",
        "pwsh",
        "sh",
        "zsh",
    }
)
ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")

# Wrapper options whose following token is data, not the wrapped executable.
# This is intentionally a bounded argv parser, not a claim that command text can
# be proven safe in general. It covers ordinary env/sudo/time/xargs spellings
# that would otherwise turn a direct SQLx reversal into a one-token bypass.
WRAPPER_VALUE_OPTIONS: dict[str, frozenset[str]] = {
    "env": frozenset({"-C", "--chdir", "-S", "--split-string", "-u", "--unset"}),
    "sudo": frozenset(
        {
            "-A",
            "-a",
            "-C",
            "-D",
            "-g",
            "-h",
            "-p",
            "-R",
            "-r",
            "-t",
            "-T",
            "-u",
            "--chdir",
            "--close-from",
            "--group",
            "--host",
            "--prompt",
            "--role",
            "--type",
            "--user",
        }
    ),
    "time": frozenset({"-f", "--format", "-o", "--output"}),
    "xargs": frozenset(
        {
            "-a",
            "--arg-file",
            "-d",
            "--delimiter",
            "-E",
            "--eof",
            "-I",
            "--replace",
            "-L",
            "--max-lines",
            "-n",
            "--max-args",
            "-P",
            "--max-procs",
            "-s",
            "--max-chars",
        }
    ),
}


def shell_tokens(segment: str) -> list[str]:
    """Tokenize one shell segment without confusing quoted policy text for code."""
    segment = WINDOWS_SEPARATOR.sub("/", segment)
    try:
        return shlex.split(segment, comments=True)
    except ValueError:
        return segment.split()


def wrapped_executable(tokens: list[str]) -> int | None:
    """Return the executable index after common transparent argv wrappers."""
    index = 0
    while index < len(tokens):
        token = tokens[index]
        if token == "--" or ASSIGNMENT.match(token):
            index += 1
            continue
        if token.startswith("-"):
            index += 1
            continue

        name = os.path.basename(token.replace("\\", "/")).casefold()
        if name not in TRANSPARENT_WRAPPERS:
            return index

        index += 1
        value_options = WRAPPER_VALUE_OPTIONS.get(name, frozenset())
        while index < len(tokens):
            option = tokens[index]
            if option == "--":
                index += 1
                break
            if name == "env" and ASSIGNMENT.match(option):
                index += 1
                continue
            if not option.startswith("-"):
                break

            option_name = option.split("=", 1)[0]
            consumes_value = option_name in value_options and "=" not in option
            index += 2 if consumes_value and index + 1 < len(tokens) else 1
    return None


def nested_shell_command(tokens: list[str], executable_index: int) -> str | None:
    """Return a literal command passed to a recognized shell interpreter."""
    executable = os.path.basename(
        tokens[executable_index].replace("\\", "/")
    ).casefold()
    if executable not in SHELL_INTERPRETERS:
        return None

    arguments = tokens[executable_index + 1 :]
    for index, argument in enumerate(arguments):
        lowered = argument.casefold()
        is_command_flag = (
            lowered in {"-c", "-command", "/c"}
            or (
                executable in {"bash", "dash", "ksh", "sh", "zsh"}
                and lowered.startswith("-")
                and "c" in lowered[1:]
            )
        )
        if is_command_flag and index + 1 < len(arguments):
            if executable in {"cmd", "cmd.exe", "powershell", "powershell.exe", "pwsh"}:
                return " ".join(arguments[index + 1 :])
            return arguments[index + 1]
    return None


def env_split_command(tokens: list[str]) -> str | None:
    """Return the command encoded by GNU/BSD env's split-string option."""
    executable_index = next(
        (
            index
            for index, token in enumerate(tokens)
            if token != "--" and not ASSIGNMENT.match(token)
        ),
        None,
    )
    if executable_index is None:
        return None
    executable = os.path.basename(
        tokens[executable_index].replace("\\", "/")
    ).casefold()
    if executable != "env":
        return None
    for index, token in enumerate(
        tokens[executable_index + 1 :], start=executable_index + 1
    ):
        if token in {"-S", "--split-string"} and index + 1 < len(tokens):
            return tokens[index + 1]
        if token.startswith("--split-string="):
            return token.split("=", 1)[1]
    return None


def is_sqlx_migration_revert(command: str) -> bool:
    """Recognize the forbidden SQLx subcommand across common argv spellings."""
    for segment in SHELL_SEGMENT.split(command):
        tokens = shell_tokens(segment)
        split_command = env_split_command(tokens)
        if split_command is not None and is_sqlx_migration_revert(split_command):
            return True
        executable_index = wrapped_executable(tokens)
        if executable_index is None:
            continue

        nested = nested_shell_command(tokens, executable_index)
        if nested is not None and is_sqlx_migration_revert(nested):
            return True

        name = os.path.basename(
            tokens[executable_index].replace("\\", "/")
        ).casefold()
        if name not in {"sqlx", "sqlx.exe"}:
            continue
        arguments = [token.casefold() for token in tokens[executable_index + 1 :]]
        if any(
            left == "migrate" and right == "revert"
            for left, right in zip(arguments, arguments[1:])
        ):
            return True
    return False


def migration_revert_refusal() -> str:
    return (
        "BLOCKED: sqlx migrate revert violates this repository's forward-only "
        "migration policy. Add the next corrective migration instead "
        "(01-conventions.md §9)."
    )


def fail_open(detail: str) -> int:
    """Warn Codex through the supported hook channel, then allow the tool call."""
    print(
        json.dumps(
            {
                "systemMessage": (
                    "WARNING: immutable-path hook failed open: "
                    f"{detail}. Git hooks and CI remain the backstops."
                )
            }
        )
    )
    return 0


def patch_paths(command: str) -> list[str]:
    """Return paths that apply_patch may create, replace, move, or delete."""
    return [
        match.group(1).replace("\\", "/")
        for line in command.splitlines()
        if (match := PATCH_PATH.match(line))
    ]


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except ValueError as exc:
        return fail_open(f"invalid JSON input ({exc})")

    tool_name = payload.get("tool_name")
    if tool_name not in {"Bash", "apply_patch"}:
        return 0
    tool_input = payload.get("tool_input")
    cwd = payload.get("cwd") or os.getcwd()
    if not isinstance(tool_input, dict) or not isinstance(cwd, str):
        return fail_open("unexpected tool_input or cwd shape")
    command = tool_input.get("command")
    if not isinstance(command, str):
        return fail_open("tool_input.command is missing or is not text")

    # Include cwd in the relevance gate: Codex can start below the repository
    # root and send ``rm 0001_init.sql`` or a patch header with only that name.
    relevance = f"{cwd}\n{command}".casefold()
    if not any(hint in relevance for hint in RELEVANT):
        return 0

    policy_path = Path(__file__).resolve().parents[2] / ".claude/hooks/protect-immutable.py"
    policy = runpy.run_path(str(policy_path))
    repo_root = policy.get("repo_root")
    check_shell = policy.get("check_shell")
    to_relative = policy.get("to_relative")
    refusal = policy.get("refusal")
    if not all(callable(item) for item in (repo_root, check_shell, to_relative, refusal)):
        return fail_open("shared immutable-path policy has an incompatible interface")

    root = repo_root(cwd)
    if not isinstance(root, Path):
        return fail_open("the active git worktree could not be resolved")
    root = root.resolve()
    effective_cwd = str(Path(cwd).resolve())

    if tool_name == "Bash":
        reason = (
            migration_revert_refusal()
            if is_sqlx_migration_revert(command)
            else check_shell(root, effective_cwd, command)
        )
    else:
        reason = None
        paths = patch_paths(command)
        if not paths:
            return fail_open("apply_patch input has no recognizable file headers")
        for raw in paths:
            relative = to_relative(root, raw, effective_cwd)
            if not isinstance(relative, str):
                return fail_open(f"the patch path {raw!r} could not be resolved")
            reason = refusal(root, relative)
            if reason:
                break

    if reason:
        if tool_name == "apply_patch":
            reason = f"{reason}\n(Detected in an apply_patch file header.)"
        print(reason, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001 - a guard error must not brick Codex
        sys.exit(fail_open(f"internal error ({exc})"))
