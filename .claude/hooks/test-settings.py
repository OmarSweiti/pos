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

    def test_sandbox_cannot_be_disabled(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["enabled"] = False
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("sandbox.enabled", result.stderr)

    def test_sandbox_startup_failure_must_refuse_closed(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["failIfUnavailable"] = False
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("failIfUnavailable", result.stderr)

    def test_project_root_is_not_added_to_allow_read(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["filesystem"]["allowRead"].append(".")
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("reviewed toolchain and template paths", result.stderr)

    def test_tracked_env_examples_remain_readable_policy_inputs(self) -> None:
        permission_denies = SETTINGS["permissions"]["deny"]
        sandbox_denies = SETTINGS["sandbox"]["filesystem"]["denyRead"]
        sandbox_allows = SETTINGS["sandbox"]["filesystem"]["allowRead"]
        self.assertNotIn("Read(//**/.env.*)", permission_denies)
        self.assertIn("./**/.env.*", sandbox_denies)
        self.assertIn("./apps/server/.env.example", sandbox_allows)
        self.assertTrue((ROOT / "apps" / "server" / ".env.example").is_file())

    def test_arbitrary_env_suffix_sandbox_deny_is_required(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["filesystem"]["denyRead"].remove("./**/.env.*")
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("protect home and secret paths", result.stderr)

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

    def test_project_cannot_widen_sandbox_write_root(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["filesystem"]["allowWrite"] = ["~/"]
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("write root", result.stderr)

    def test_unsandboxed_escape_hatch_cannot_be_enabled(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["allowUnsandboxedCommands"] = True
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("allowUnsandboxedCommands", result.stderr)

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

    def test_command_network_allowlist_must_stay_explicitly_empty(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        del changed["sandbox"]["network"]["allowedDomains"]
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("allowlist", result.stderr)

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
        changed["sandbox"]["enabled"] = False
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
                check=False,
            )
        self.assertEqual(result.returncode, 2)
        self.assertIn("sandbox.enabled", result.stderr)

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

    def test_missing_credential_deny_is_rejected(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["credentials"]["envVars"] = [
            item
            for item in changed["sandbox"]["credentials"]["envVars"]
            if item["name"] != "DATABASE_URL"
        ]
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("credential envVars", result.stderr)

    def test_conflicting_credential_entry_is_rejected(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["credentials"]["envVars"].append(
            {"name": "DATABASE_URL", "mode": "mask"}
        )
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("credential envVars", result.stderr)

    def test_missing_sensitive_file_deny_is_rejected(self) -> None:
        changed = copy.deepcopy(SETTINGS)
        changed["sandbox"]["filesystem"]["denyRead"].remove("./**/.env")
        temporary, root = self.fixture(changed)
        with temporary:
            result = validate(root)
        self.assertEqual(result.returncode, 2)
        self.assertIn("protect home and secret paths", result.stderr)

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
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("BLOCKED", result.stderr)

    def test_local_settings_cannot_weaken_sandbox(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({"sandbox": {"filesystem": {"disabled": True}}}),
                encoding="utf-8",
            )
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 2)
        self.assertIn("filesystem isolation", result.stderr)

    def test_local_settings_cannot_make_sandbox_startup_fail_open(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({"sandbox": {"failIfUnavailable": False}}),
                encoding="utf-8",
            )
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 2)
        self.assertIn("fail open", result.stderr)

    def test_local_settings_cannot_add_mach_lookup_access(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({"sandbox": {"network": {"allowMachLookup": ["com.apple.securityd"]}}}),
                encoding="utf-8",
            )
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 2)
        self.assertIn("Mach/XPC", result.stderr)

    def test_local_settings_cannot_autoapprove_sandboxed_commands(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({"sandbox": {"autoAllowBashIfSandboxed": True}}),
                encoding="utf-8",
            )
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 2)
        self.assertIn("auto-approve", result.stderr)

    def test_local_settings_cannot_reopen_home_directory(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({"sandbox": {"filesystem": {"allowRead": ["~/"]}}}),
                encoding="utf-8",
            )
            result = validate(root, "local_settings", local)
        self.assertEqual(result.returncode, 2)
        self.assertIn("widen sandbox read access", result.stderr)

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

    def test_local_deny_arrays_only_tighten_and_are_allowed(self) -> None:
        temporary, root = self.fixture()
        with temporary:
            local = root / ".claude" / "settings.local.json"
            local.write_text(
                json.dumps({
                    "permissions": {
                        "ask": ["Bash(git push *)"],
                        "deny": ["Read(~/another-secret)"],
                    },
                    "sandbox": {
                        "filesystem": {
                            "denyRead": ["~/another-secret"],
                            "denyWrite": ["~/protected"],
                        },
                        "network": {"deniedDomains": ["example.invalid"]},
                        "credentials": {
                            "envVars": [{"name": "EXTRA_TOKEN", "mode": "deny"}],
                        },
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
