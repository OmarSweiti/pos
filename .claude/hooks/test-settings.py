#!/usr/bin/env python3
"""Negative tests for Claude settings and the ConfigChange guard."""

from __future__ import annotations

import copy
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SETTINGS = json.loads((ROOT / ".claude" / "settings.json").read_text(encoding="utf-8"))
VALIDATOR = HERE / "validate-settings.py"
LAUNCHER = HERE / "run-python-hook.mjs"


def write_settings(root: Path, settings: object) -> Path:
    target = root / ".claude" / "settings.json"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(settings), encoding="utf-8")
    return target


def validate(root: Path, source: str = "project_settings", file_path: Path | None = None) -> subprocess.CompletedProcess[str]:
    payload = {
        "hook_event_name": "ConfigChange",
        "source": source,
        "cwd": str(root),
        "file_path": str(file_path or root / ".claude" / "settings.json"),
    }
    return subprocess.run(
        ["python3", str(VALIDATOR)],
        input=json.dumps(payload),
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )


class SettingsContract(unittest.TestCase):
    def fixture(self, settings: object = SETTINGS) -> tuple[tempfile.TemporaryDirectory[str], Path]:
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        write_settings(root, settings)
        return temporary, root

    def test_checked_in_settings_satisfy_guard(self) -> None:
        result = validate(ROOT)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_checked_in_skills_satisfy_configchange_guard(self) -> None:
        result = validate(
            ROOT,
            "skills",
            ROOT / ".claude" / "skills" / "add-migration" / "SKILL.md",
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_repository_local_settings_do_not_weaken_policy_when_present(self) -> None:
        local = ROOT / ".claude" / "settings.local.json"
        if not local.exists():
            return
        result = validate(ROOT, "local_settings", local)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_official_settings_schema_is_required(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        del changed["$schema"]
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("$schema", result.stderr)

    def test_project_cannot_add_autoapproved_commands(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["permissions"]["allow"].append("Bash(git push:*)")
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("reviewed command set", result.stderr)

    def test_sandbox_cannot_be_reenabled(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["enabled"] = True
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("intentionally disabled", result.stderr)

    def test_sandbox_enabled_rejects_integer_false(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["enabled"] = 0
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("intentionally disabled", result.stderr)

    def test_tracked_env_examples_remain_readable_policy_inputs(self) -> None:
        permission_denies = SETTINGS["permissions"]["deny"]
        self.assertNotIn("Read(//**/.env.*)", permission_denies)
        self.assertIn("Read(//**/.env)", permission_denies)
        self.assertTrue((ROOT / "apps" / "server" / ".env.example").is_file())

    def test_pretool_launcher_and_read_matcher_remain_fail_closed(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["hooks"]["PreToolUse"][0]["matcher"] = (
            "Read|Edit|Write|NotebookEdit|Bash|PowerShell|Monitor"
        )
        changed["hooks"]["PreToolUse"][0]["hooks"][0]["args"].remove(
            "--fail-closed"
        )
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("PreToolUse", result.stderr)

    def test_repository_hooks_cannot_be_disabled(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["disableAllHooks"] = True
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("disableAllHooks", result.stderr)

    def test_project_cannot_widen_working_directory(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["permissions"]["additionalDirectories"] = ["../"]
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("working directories", result.stderr)

    def test_project_sandbox_shape_is_exact(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["filesystem"]["allowWrite"] = ["~/"]
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("only the dormant", result.stderr)

    def test_bypass_permissions_cannot_be_reenabled(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        del changed["permissions"]["disableBypassPermissionsMode"]
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("disableBypassPermissionsMode", result.stderr)

    def test_manual_permission_mode_cannot_be_removed(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        del changed["permissions"]["defaultMode"]
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("defaultMode", result.stderr)

    def test_powershell_cannot_be_dropped_from_pretool_matcher(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["hooks"]["PreToolUse"][0]["matcher"] = "Edit|Write|Bash|Monitor"
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("PreToolUse", result.stderr)

    def test_shell_form_hook_cannot_replace_exec_form(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        handler = changed["hooks"]["PostToolUse"][0]["hooks"][0]
        handler.pop("args")
        handler["command"] = 'python3 "$CLAUDE_PROJECT_DIR/hook.py"'
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("PostToolUse", result.stderr)

    def test_exec_launcher_preserves_configchange_block(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["enabled"] = True
        temporary, root = self.fixture(changed)
        with temporary:
            payload = {
                "hook_event_name": "ConfigChange",
                "source": "project_settings",
                "cwd": str(root),
                "file_path": str(root / ".claude" / "settings.json"),
            }
            result = subprocess.run(
                ["node", str(LAUNCHER), "--fail-closed", str(VALIDATOR)],
                input=json.dumps(payload),
                capture_output=True,
                text=True,
                encoding="utf-8",
                check=False,
            )
        self.assertEqual(result.returncode, 2)
        self.assertIn("intentionally disabled", result.stderr)

    def test_configchange_launcher_fails_closed_without_python(self) -> None:
        node = shutil.which("node")
        self.assertIsNotNone(node, "Node is required by the repository hook launcher")
        temporary, root = self.fixture()
        with temporary:
            empty_path = root / "empty-path"
            empty_path.mkdir()
            payload = {
                "hook_event_name": "ConfigChange",
                "source": "project_settings",
                "cwd": str(root),
                "file_path": str(root / ".claude" / "settings.json"),
            }
            environment = os.environ.copy()
            environment["PATH"] = str(empty_path)
            result = subprocess.run(
                [str(node), str(LAUNCHER), "--fail-closed", str(VALIDATOR)],
                input=json.dumps(payload),
                capture_output=True,
                text=True,
                encoding="utf-8",
                check=False,
                env=environment,
            )
        self.assertEqual(result.returncode, 2)
        self.assertIn("No Python 3 interpreter", result.stderr)

    def test_configchange_cannot_drop_fail_closed_launcher_mode(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        arguments = changed["hooks"]["ConfigChange"][0]["hooks"][0]["args"]
        arguments.remove("--fail-closed")
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("ConfigChange", result.stderr)

    def test_missing_sensitive_file_deny_is_rejected(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["permissions"]["deny"].remove("Read(~/.ssh/**)")
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("protected-path and secret denies", result.stderr)

    def test_malformed_project_settings_are_blocked(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            (root / ".claude" / "settings.json").write_text("{", encoding="utf-8")
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("invalid or unreadable JSON", result.stderr)

    def test_malformed_hook_payload_fails_closed(self) -> None:
        result = subprocess.run(
            ["python3", str(VALIDATOR)],
            input="not json",
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("BLOCKED", result.stderr)

    def test_local_settings_cannot_override_disabled_sandbox_mode(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({"sandbox": {"enabled": True}}),
                encoding="utf-8",
            )
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 2)
        self.assertIn("reviewed disabled sandbox mode", result.stderr)

    def test_local_settings_cannot_add_autoapproved_commands(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({"permissions": {"allow": ["Bash(echo ok)"]}}),
                encoding="utf-8",
            )
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 2)
        self.assertIn("auto-approved", result.stderr)

    def test_local_settings_cannot_change_default_permission_mode(self) -> None:
        for mode in ("acceptEdits", "dontAsk", "bypassPermissions"):
            with self.subTest(mode=mode):
                temporary, root = self.fixture()
                with temporary:
                    local = root / ".claude" / "settings.local.json"
                    local.write_text(
                        json.dumps({"permissions": {"defaultMode": mode}}),
                        encoding="utf-8",
                    )
                    result = validate(root, "local_settings", local)
                self.assertEqual(result.returncode, 2)
                self.assertIn("permission mode", result.stderr)

    def test_local_settings_cannot_add_working_directories(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({"permissions": {"additionalDirectories": ["../"]}}),
                encoding="utf-8",
            )
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 2)
        self.assertIn("working directories", result.stderr)

    def test_local_settings_cannot_override_hooks(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(json.dumps({"disableAllHooks": True}), encoding="utf-8")
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 2)
        self.assertIn("repository hooks", result.stderr)

    def test_extra_autoapproving_hook_is_rejected(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["hooks"]["PermissionRequest"] = [{
            "hooks": [{"type": "command", "command": "true", "args": []}],
        }]
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("only the reviewed", result.stderr)

    def test_local_permission_denies_and_prompts_are_allowed(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({
                    "permissions": {
                        "ask": ["Bash(git push *)"],
                        "deny": ["Read(~/another-secret)"],
                    },
                }),
                encoding="utf-8",
            )
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_empty_local_settings_do_not_weaken_policy(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text("{}", encoding="utf-8")
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
