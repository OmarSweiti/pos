#!/usr/bin/env python3
"""ConfigChange guard for the repository's minimum Claude security posture."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

LAUNCHER = "${CLAUDE_PROJECT_DIR}/.claude/hooks/run-python-hook.mjs"
HOOKS = {
    "PreToolUse": (
        "Read|Grep|Glob|Edit|Write|NotebookEdit|Bash|PowerShell|Monitor",
        "${CLAUDE_PROJECT_DIR}/.claude/hooks/protect-immutable.py",
        15,
    ),
    "PostToolUse": (
        "Edit|Write|NotebookEdit|Bash|PowerShell|Monitor",
        "${CLAUDE_PROJECT_DIR}/.claude/hooks/docs-links-on-write.py",
        30,
    ),
    "ConfigChange": (
        "project_settings|local_settings|skills",
        "${CLAUDE_PROJECT_DIR}/.claude/hooks/validate-settings.py",
        15,
    ),
}
FAIL_CLOSED_HOOK_EVENTS = {"PreToolUse", "ConfigChange"}
APPROVED_PERMISSION_ALLOWS = {
    "Bash(just lint)",
    "Bash(just test)",
    "Bash(just fmt)",
    "Bash(just acyclic)",
    "Bash(just docs-links)",
    "Bash(just verify-schema)",
    "Bash(just guards)",
    "Bash(cargo fmt:*)",
    "Bash(cargo clippy:*)",
    "Bash(cargo check:*)",
    "Bash(cargo nextest run:*)",
    "Bash(cargo tree:*)",
    "Bash(cargo metadata:*)",
    "Bash(pnpm biome ci:*)",
    "Bash(pnpm biome check:*)",
    "Bash(pnpm -r --if-present test)",
    "Bash(./scripts/check-doc-links.sh)",
    "Bash(./scripts/check-domain-acyclic.py)",
    "Bash(./scripts/verify-schema.py:*)",
    "Bash(./scripts/verify-pg-migrations.py:*)",
    "Bash(./.claude/hooks/test-protect-immutable.sh)",
    "Bash(./.claude/hooks/test-docs-links.sh)",
    "Bash(./.githooks/test-hooks.sh)",
    "Bash(git status:*)",
    "Bash(git diff:*)",
    "Bash(git log:*)",
    "Bash(git show:*)",
    "Bash(git ls-files:*)",
    "Bash(git ls-tree:*)",
    "Bash(git branch)",
    "Bash(git branch --show-current)",
    "Bash(git rev-parse:*)",
}
REQUIRED_PERMISSION_DENIES = {
    "Edit(/docs/plan/**)",
    "Read(//**/.env)",
    "Read(//**/.env.local)",
    "Read(//**/.env.development)",
    "Read(//**/.env.development.local)",
    "Read(//**/.env.test)",
    "Read(//**/.env.test.local)",
    "Read(//**/.env.staging)",
    "Read(//**/.env.production)",
    "Read(//**/.env.production.local)",
    "Read(//**/*.sqlite)",
    "Read(//**/*.sqlite3)",
    "Read(//**/*.db)",
    "Read(//**/*.db-wal)",
    "Read(//**/*.db-shm)",
    "Read(//**/*.sqlite-wal)",
    "Read(//**/*.sqlite-shm)",
    "Read(//**/*.pem)",
    "Read(//**/*.key)",
    "Read(//**/*.p12)",
    "Read(//**/*.pfx)",
    "Read(//**/*.jks)",
    "Read(//**/*.keystore)",
    "Read(//**/id_rsa)",
    "Read(//**/id_ed25519)",
    "Read(//**/.npmrc)",
    "Read(//**/.netrc)",
    "Read(//**/_netrc)",
    "Read(//**/.git-credentials)",
    "Read(//**/credentials)",
    "Read(//**/credentials.toml)",
    "Read(//**/.docker/config.json)",
    "Read(~/.ssh/**)",
    "Read(~/.gnupg/**)",
    "Read(~/.aws/**)",
    "Read(~/.azure/**)",
    "Read(~/.kube/**)",
    "Read(~/.config/gh/**)",
    "Read(~/.config/gcloud/**)",
    "Read(~/.config/op/**)",
    "Read(~/.local/share/keyrings/**)",
    "Read(~/.claude/**)",
    "Read(~/.claude.json)",
    "Read(~/.codex/**)",
    "Read(~/Library/Keychains/**)",
    "Read(~/Library/Application Support/1Password/**)",
    "Read(~/Library/Application Support/Bitwarden/**)",
}
SKILL_CONTRACTS = {
    "add-migration": (
        "never edit, delete, rename, or",
        "exact,\n  ordered parity",
        "uniquely named scratch database",
        "just lint",
        "just test",
    ),
    "verify-schema": (
        "exact ordered parity",
        "documentation/runtime",
        "uniquely named scratch database",
        "just guards",
    ),
}


def project_root(cwd: str) -> Path:
    try:
        done = subprocess.run(
            ["git", "-C", cwd, "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=5,
        )
    except (OSError, subprocess.SubprocessError):
        return Path(cwd)
    return Path(done.stdout.strip()) if done.returncode == 0 else Path(cwd)


def load_object(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    try:
        loaded = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        return None, f"{path}: invalid or unreadable JSON ({exc})"
    if not isinstance(loaded, dict):
        return None, f"{path}: top-level value must be an object"
    return loaded, None


def nested(data: dict[str, Any], *keys: str) -> Any:
    current: Any = data
    for key in keys:
        if not isinstance(current, dict) or key not in current:
            return None
        current = current[key]
    return current


def validates_hook(
    settings: dict[str, Any],
    event: str,
    matcher: str,
    script: str,
    timeout: int,
) -> bool:
    args = [LAUNCHER, script]
    if event in FAIL_CLOSED_HOOK_EVENTS:
        args.insert(1, "--fail-closed")
    expected = [{
        "matcher": matcher,
        "hooks": [{
            "type": "command",
            "command": "node",
            "args": args,
            "timeout": timeout,
        }],
    }]
    return nested(settings, "hooks", event) == expected


def project_errors(settings: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if settings.get("$schema") != "https://json.schemastore.org/claude-code-settings.json":
        errors.append("the official Claude Code settings $schema is required")
    if settings.get("disableAllHooks") is not False:
        errors.append("disableAllHooks must remain false so repository hooks stay active")
    if nested(settings, "permissions", "defaultMode") != "default":
        errors.append("permissions.defaultMode must remain 'default' (manual)")
    if nested(settings, "permissions", "disableBypassPermissionsMode") != "disable":
        errors.append("permissions.disableBypassPermissionsMode must remain 'disable'")
    if nested(settings, "permissions", "additionalDirectories"):
        errors.append("project settings cannot add filesystem working directories")

    allow = nested(settings, "permissions", "allow")
    if (
        not isinstance(allow, list)
        or not all(isinstance(rule, str) for rule in allow)
        or len(allow) != len(APPROVED_PERMISSION_ALLOWS)
        or set(allow) != APPROVED_PERMISSION_ALLOWS
    ):
        errors.append("permissions.allow must contain exactly the reviewed command set")

    deny = nested(settings, "permissions", "deny")
    if (
        not isinstance(deny, list)
        or not all(isinstance(rule, str) for rule in deny)
        or not REQUIRED_PERMISSION_DENIES.issubset(set(deny))
    ):
        errors.append("the project-anchored protected-path and secret denies are required")
    if isinstance(deny, list) and any(
        isinstance(rule, str)
        and rule.startswith(("Write(", "MultiEdit(", "NotebookEdit("))
        for rule in deny
    ):
        errors.append("file path permissions must use Read/Edit, not unsupported tool names")

    reviewed_sandbox = {
        "enabled": False,
        "filesystem": {"denyWrite": ["./docs/plan"]},
    }
    if (
        settings.get("sandbox") != reviewed_sandbox
        or nested(settings, "sandbox", "enabled") is not False
    ):
        errors.append(
            "sandbox must remain intentionally disabled with only the dormant "
            "docs/plan denyWrite default"
        )

    hook_map = settings.get("hooks")
    if not isinstance(hook_map, dict) or set(hook_map) != set(HOOKS):
        errors.append("only the reviewed repository hook events may be configured")
    for event, (matcher, script, timeout) in HOOKS.items():
        if not validates_hook(settings, event, matcher, script, timeout):
            errors.append(f"{event} must retain its portable exec-form hook")

    if nested(settings, "attribution", "commit") != "":
        errors.append("Claude commit attribution must remain disabled by repository policy")
    if nested(settings, "attribution", "pr") != "":
        errors.append("Claude PR attribution must remain disabled by repository policy")
    if nested(settings, "attribution", "sessionUrl") is not False:
        errors.append("Claude session trailers must remain disabled by repository policy")
    return errors


def local_errors(settings: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    # Permission arrays merge across scopes. Local deny/ask rules only add
    # friction, while allow rules can widen access and must remain empty.
    permissions_allow = nested(settings, "permissions", "allow")
    if isinstance(permissions_allow, list) and permissions_allow:
        errors.append("local settings cannot add auto-approved tool or command rules")
    if nested(settings, "permissions", "defaultMode") is not None:
        errors.append("local settings cannot override the repository permission mode")
    if nested(settings, "permissions", "additionalDirectories"):
        errors.append("local settings cannot add filesystem working directories")
    if nested(settings, "permissions", "skipDangerousModePermissionPrompt") is True:
        errors.append("local settings cannot suppress dangerous-mode prompts")
    bypass = nested(settings, "permissions", "disableBypassPermissionsMode")
    if bypass not in (None, "disable"):
        errors.append("local settings cannot re-enable bypassPermissions")
    if nested(settings, "sandbox", "enabled") is not None:
        errors.append("local settings cannot override the reviewed disabled sandbox mode")
    if settings.get("disableAllHooks") is True or settings.get("hooks"):
        errors.append("local settings cannot disable or override repository hooks")
    if settings.get("attribution"):
        errors.append("local settings cannot override repository attribution policy")
    return errors


def skill_errors(root: Path) -> list[str]:
    errors: list[str] = []
    for name, required in SKILL_CONTRACTS.items():
        path = root / ".claude" / "skills" / name / "SKILL.md"
        try:
            body = path.read_text(encoding="utf-8")
        except OSError as exc:
            errors.append(f"{path}: missing or unreadable ({exc})")
            continue
        if not body.startswith(f"---\nname: {name}\n"):
            errors.append(f"{path}: frontmatter name must match the skill directory")
        for phrase in required:
            if phrase not in body:
                errors.append(f"{path}: required safety contract is missing: {phrase!r}")
    return errors


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (OSError, ValueError) as exc:
        print(
            f"BLOCKED: could not validate a malformed ConfigChange payload ({exc})",
            file=sys.stderr,
        )
        return 2
    if not isinstance(payload, dict):
        print("BLOCKED: ConfigChange payload must be an object", file=sys.stderr)
        return 2

    cwd = payload.get("cwd")
    root = project_root(cwd if isinstance(cwd, str) else os.getcwd())
    project_path = root / ".claude" / "settings.json"
    project, load_error = load_object(project_path)
    errors = [load_error] if load_error else project_errors(project or {})

    if payload.get("source") == "local_settings":
        raw_path = payload.get("file_path")
        local_path = (
            Path(raw_path)
            if isinstance(raw_path, str)
            else root / ".claude" / "settings.local.json"
        )
        local, local_load_error = load_object(local_path)
        if local_load_error:
            errors.append(local_load_error)
        else:
            errors.extend(local_errors(local or {}))
    elif payload.get("source") == "skills":
        errors.extend(skill_errors(root))

    if errors:
        print("BLOCKED: Claude configuration weakens repository policy:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:  # noqa: BLE001
        print(
            f"BLOCKED: Claude settings guard failed internally ({exc})",
            file=sys.stderr,
        )
        sys.exit(2)
