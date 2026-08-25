#!/usr/bin/env python3
"""Deterministically validate the repository-scoped Codex safety policy.

This test intentionally does not call Codex or the network. The installed CLI's
strict-config mode is currently available through `codex doctor`, whose exit
status also depends on authentication, connectivity, and mutable local state.
Those are poor inputs to a repository guard.

Run the CLI's execpolicy checker separately when changing rule semantics:

    codex execpolicy check --rules .codex/rules/safety.rules -- git push
"""

from __future__ import annotations

import re
from pathlib import Path

try:
    import tomllib
except ImportError:  # pragma: no cover - depends on the host Python
    print("codex-policy: Python 3.11 or newer is required (tomllib is missing)")
    raise SystemExit(2) from None


CODEX_DIR = Path(__file__).resolve().parent
CONFIG_PATH = CODEX_DIR / "config.toml"
RULES_DIR = CODEX_DIR / "rules"
SCHEMA_DIRECTIVE = (
    "#:schema https://developers.openai.com/codex/config-schema.json"
)

# These belong to a person's or machine's ~/.codex/config.toml, not to a shared
# repository. Several provider/auth/telemetry keys are ignored project-locally
# by Codex already; testing the wider set prevents preference drift as well.
PERSONAL_OR_MACHINE_KEYS = frozenset(
    {
        "analytics",
        "chatgpt_base_url",
        "check_for_update_on_startup",
        "cli_auth_credentials_store",
        "feedback",
        "file_opener",
        "forced_chatgpt_workspace_id",
        "forced_login_method",
        "history",
        "model",
        "model_provider",
        "model_providers",
        "model_reasoning_effort",
        "notify",
        "openai_base_url",
        "otel",
        "profile",
        "profiles",
        "review_model",
        "service_tier",
        "tui",
    }
)

# This is the complete reviewed escalation surface. Exact-prefix rules are not
# a shell parser or a sandbox boundary, but deleting one must not silently turn
# a consequential outside-sandbox operation into an unreviewed command.
REQUIRED_RULE_DECISIONS = {
    '["git", ["commit", "merge", "rebase", "cherry-pick", "tag"]]': "prompt",
    '["git", ["reset", "clean", "restore"]]': "prompt",
    '["git", "branch", "-D"]': "prompt",
    '["git", "stash", ["drop", "clear"]]': "prompt",
    '["git", "push"]': "prompt",
    '["gh", "pr", ["create", "merge", "close", "reopen", "ready", "edit"]]': "prompt",
    '["gh", "release", ["create", "edit", "delete", "upload"]]': "prompt",
    '["gh", "workflow", ["run", "enable", "disable"]]': "prompt",
    '["gh", "repo", ["archive", "delete", "edit", "rename"]]': "prompt",
    '["just", ["pr", "promote-staging", "promote-main", "gh-bootstrap", "gh-actions-policy", "gh-protect", "gh-project"]]': "prompt",
    '["just", ["migrate", "db-reset", "db-local-reset"]]': "prompt",
    '["sqlx", "migrate", "revert"]': "forbidden",
    '["sqlx", "database", "drop"]': "prompt",
    '[["cargo", "pnpm", "npm"], "publish"]': "prompt",
}


def require(condition: bool, message: str, errors: list[str]) -> None:
    if not condition:
        errors.append(message)


def load_config(errors: list[str]) -> tuple[dict[str, object], str]:
    try:
        source = CONFIG_PATH.read_text(encoding="utf-8")
    except OSError as error:
        errors.append(f"cannot read {CONFIG_PATH.relative_to(CODEX_DIR.parent)}: {error}")
        return {}, ""

    require(
        source.startswith(SCHEMA_DIRECTIVE + "\n"),
        f"{CONFIG_PATH.relative_to(CODEX_DIR.parent)} must start with the official schema directive",
        errors,
    )
    try:
        return tomllib.loads(source), source
    except tomllib.TOMLDecodeError as error:
        errors.append(f"invalid TOML in {CONFIG_PATH.relative_to(CODEX_DIR.parent)}: {error}")
        return {}, source


def validate_config(config: dict[str, object], errors: list[str]) -> None:
    require(
        config.get("approval_policy") == "on-request",
        'approval_policy must be "on-request"',
        errors,
    )
    require(
        config.get("approvals_reviewer") == "user",
        'approvals_reviewer must be "user"',
        errors,
    )
    require(
        config.get("sandbox_mode") == "workspace-write",
        'sandbox_mode must be "workspace-write"',
        errors,
    )
    require(
        config.get("allow_login_shell") is False,
        "allow_login_shell must be false",
        errors,
    )
    require(
        "default_permissions" not in config and "permissions" not in config,
        "do not combine the audited sandbox policy with permission profiles",
        errors,
    )

    sandbox = config.get("sandbox_workspace_write", {})
    require(
        isinstance(sandbox, dict),
        "sandbox_workspace_write must be a TOML table",
        errors,
    )
    if isinstance(sandbox, dict):
        # Pinned to the reviewed value rather than to "off". Reading the world —
        # `cargo fetch`, `pnpm install`, `gh pr view`, `git fetch` — is allowed
        # inside the sandbox; the mutating commands stay `prompt` in
        # .codex/rules/safety.rules, which is where the boundary actually lives.
        # Asserting True still catches a silent change in either direction.
        require(
            sandbox.get("network_access") is True,
            "sandbox_workspace_write.network_access must be true (reviewed: reads "
            "are free, mutations stay approval-gated in safety.rules)",
            errors,
        )
        require(
            not sandbox.get("writable_roots"),
            "sandbox_workspace_write.writable_roots must not expand beyond the repository",
            errors,
        )

    environment = config.get("shell_environment_policy", {})
    require(
        isinstance(environment, dict),
        "shell_environment_policy must be a TOML table",
        errors,
    )
    if isinstance(environment, dict):
        require(
            environment.get("ignore_default_excludes") is False,
            "shell_environment_policy.ignore_default_excludes must be false",
            errors,
        )
        filters = environment.get("filters", {})
        require(
            isinstance(filters, dict),
            "shell_environment_policy.filters must be a TOML table",
            errors,
        )
        if isinstance(filters, dict):
            normalized = {str(key).casefold(): value for key, value in filters.items()}
            expected = {
                "*password*": "exclude",
                "*credential*": "exclude",
                "database_url": "exclude",
            }
            for pattern, decision in expected.items():
                require(
                    normalized.get(pattern) == decision,
                    f'shell environment filter {pattern!r} must be "{decision}"',
                    errors,
                )

    features = config.get("features", {})
    require(isinstance(features, dict), "features must be a TOML table", errors)
    if isinstance(features, dict):
        require(
            features.get("hooks") is True,
            "features.hooks must be true so repository guards cannot be disabled by user defaults",
            errors,
        )

    misplaced = sorted(PERSONAL_OR_MACHINE_KEYS.intersection(config))
    require(
        not misplaced,
        "move personal or machine-local keys to ~/.codex/config.toml: "
        + ", ".join(misplaced),
        errors,
    )


def validate_rules(errors: list[str]) -> None:
    rule_paths = sorted(RULES_DIR.glob("*.rules"))
    require(bool(rule_paths), "at least one .codex/rules/*.rules file is required", errors)

    combined = ""
    actual_rule_decisions: dict[str, str] = {}
    for path in rule_paths:
        try:
            source = path.read_text(encoding="utf-8")
        except OSError as error:
            errors.append(f"cannot read {path.relative_to(CODEX_DIR.parent)}: {error}")
            continue

        combined += "\n" + source
        call_count = len(re.findall(r"(?m)^prefix_rule\s*\(", source))
        decisions = re.findall(
            r'(?m)^\s*decision\s*=\s*["\'](allow|prompt|forbidden)["\']\s*,?$',
            source,
        )
        justifications = re.findall(r"(?m)^\s*justification\s*=", source)
        matches = re.findall(r"(?m)^\s*match\s*=", source)
        non_matches = re.findall(r"(?m)^\s*not_match\s*=", source)

        relative = path.relative_to(CODEX_DIR.parent)
        require(call_count > 0, f"{relative} contains no prefix_rule calls", errors)
        require(
            len(decisions) == call_count,
            f"every prefix_rule in {relative} must have one explicit decision",
            errors,
        )
        require(
            len(justifications) == call_count,
            f"every prefix_rule in {relative} must explain its justification",
            errors,
        )
        require(
            len(matches) == call_count and len(non_matches) == call_count,
            f"every prefix_rule in {relative} must include match and not_match tests",
            errors,
        )
        require(
            "allow" not in decisions,
            f"{relative} must not grant unsandboxed execution with an allow decision",
            errors,
        )

        blocks = re.findall(r"(?ms)^prefix_rule\(\n(.*?)^\)\s*$", source)
        require(
            len(blocks) == call_count,
            f"every prefix_rule in {relative} must use the reviewed block form",
            errors,
        )
        for block in blocks:
            pattern_match = re.search(
                r"(?m)^\s*pattern\s*=\s*(.+),\s*$", block
            )
            decision_match = re.search(
                r'(?m)^\s*decision\s*=\s*["\'](allow|prompt|forbidden)["\']\s*,?\s*$',
                block,
            )
            if pattern_match is None or decision_match is None:
                continue
            pattern = pattern_match.group(1).strip()
            if pattern in actual_rule_decisions:
                errors.append(f"{relative} duplicates reviewed pattern {pattern}")
                continue
            actual_rule_decisions[pattern] = decision_match.group(1)

    missing = sorted(set(REQUIRED_RULE_DECISIONS) - set(actual_rule_decisions))
    extra = sorted(set(actual_rule_decisions) - set(REQUIRED_RULE_DECISIONS))
    changed = sorted(
        pattern
        for pattern in set(REQUIRED_RULE_DECISIONS).intersection(actual_rule_decisions)
        if REQUIRED_RULE_DECISIONS[pattern] != actual_rule_decisions[pattern]
    )
    require(
        not missing,
        "reviewed execpolicy rule(s) are missing: " + "; ".join(missing),
        errors,
    )
    require(
        not extra,
        "unreviewed execpolicy rule(s) were added: " + "; ".join(extra),
        errors,
    )
    require(
        not changed,
        "reviewed execpolicy decision(s) changed: " + "; ".join(changed),
        errors,
    )

    require(
        'pattern = ["sqlx", "migrate", "revert"]' in combined
        and re.search(
            r'pattern\s*=\s*\["sqlx",\s*"migrate",\s*"revert"\][\s\S]*?decision\s*=\s*"forbidden"',
            combined,
        )
        is not None,
        "canonical sqlx migrate revert escalation must remain forbidden; the PreToolUse hook covers argv variants",
        errors,
    )


def main() -> int:
    errors: list[str] = []
    config, _source = load_config(errors)
    if config:
        validate_config(config, errors)
    validate_rules(errors)

    if errors:
        for error in errors:
            print(f"codex-policy: FAIL: {error}")
        return 1

    print("codex-policy: config and execpolicy invariants passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
