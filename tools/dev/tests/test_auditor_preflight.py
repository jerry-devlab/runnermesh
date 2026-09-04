import json
import tempfile
import unittest
from pathlib import Path
from typing import Mapping, Sequence
from unittest import mock

from tools.dev.auditor_preflight import (
    CHILD_MARKER,
    CommandResult,
    build_command,
    parse_exec_events,
    remove_windowsapps_from_path,
    resolve_inbox_powershell,
    run_preflight,
    validate_profile_contract,
)


class FakeRunner:
    def __init__(self, result: CommandResult):
        self.result = result
        self.calls: list[tuple[tuple[str, ...], Mapping[str, str]]] = []

    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str],
        timeout: float,
    ) -> CommandResult:
        self.calls.append((tuple(args), dict(env)))
        return self.result


def event(event_type: str, item: dict | None = None) -> str:
    value: dict[str, object] = {"type": event_type}
    if item is not None:
        value["item"] = item
    return json.dumps(value)


INBOX_POWERSHELL = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"


def successful_output() -> str:
    return "\n".join(
        [
            event("thread.started"),
            event(
                "item.completed",
                {
                    "type": "command_execution",
                    "command": (
                        f'"{INBOX_POWERSHELL}" -NoProfile -Command '
                        f'"Write-Output \'{CHILD_MARKER}\'; exit 0"'
                    ),
                    "exit_code": 0,
                    "aggregated_output": f"{CHILD_MARKER}\n",
                },
            ),
        ]
    )


class AuditorPreflightTests(unittest.TestCase):
    def write_profile(self, root: Path, sandbox: str = "read-only") -> Path:
        profile = root / "jerry-auditor.config.toml"
        profile.write_text(
            f'sandbox_mode = "{sandbox}"\napproval_policy = "never"\n',
            encoding="utf-8",
        )
        return profile

    def test_windowsapps_resolution_is_removed_only_from_child_path(self) -> None:
        original = ";".join(
            [r"C:\Tools", "C:/Program Files/WindowsApps", r"C:\Windows"]
        )
        sanitized, removed = remove_windowsapps_from_path(original)
        self.assertTrue(removed)
        self.assertEqual(sanitized, ";".join([r"C:\Tools", r"C:\Windows"]))
        self.assertIn("WindowsApps", original)

    def test_only_existing_inbox_powershell_is_resolved(self) -> None:
        environment = {"SystemRoot": r"C:\Windows"}
        with mock.patch("tools.dev.auditor_preflight.os.path.isfile", return_value=True):
            self.assertEqual(resolve_inbox_powershell(environment), INBOX_POWERSHELL)
        with mock.patch("tools.dev.auditor_preflight.os.path.isfile", return_value=False):
            self.assertIsNone(resolve_inbox_powershell(environment))
        self.assertIsNone(resolve_inbox_powershell({}))

    def test_profile_contract_requires_read_only_and_never(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            validate_profile_contract(self.write_profile(root))
            with self.assertRaisesRegex(ValueError, "read-only"):
                validate_profile_contract(self.write_profile(root, "workspace-write"))

    def test_event_parser_requires_exactly_one_successful_command_item(self) -> None:
        evidence = parse_exec_events(successful_output(), INBOX_POWERSHELL)
        self.assertTrue(evidence.profile_start)
        self.assertTrue(evidence.child_process_admission)
        self.assertEqual(evidence.command_count, 1)

        hallucinated = "\n".join(
            [
                event("thread.started"),
                event("item.completed", {"type": "agent_message", "text": CHILD_MARKER}),
            ]
        )
        self.assertFalse(
            parse_exec_events(hallucinated, INBOX_POWERSHELL).child_process_admission
        )
        self.assertFalse(
            parse_exec_events(
                successful_output() + "\n" + successful_output(), INBOX_POWERSHELL
            ).child_process_admission
        )

        wrong_command = successful_output().replace("Write-Output", "Get-ChildItem")
        self.assertFalse(
            parse_exec_events(wrong_command, INBOX_POWERSHELL).child_process_admission
        )
        prepended_statement = successful_output().replace(
            "Write-Output", "Get-ChildItem; Write-Output"
        )
        self.assertFalse(
            parse_exec_events(
                prepended_statement, INBOX_POWERSHELL
            ).child_process_admission
        )
        no_thread = "\n".join(successful_output().splitlines()[1:])
        self.assertFalse(
            parse_exec_events(no_thread, INBOX_POWERSHELL).child_process_admission
        )
        extra_tool = successful_output() + "\n" + event(
            "item.completed", {"type": "web_search"}
        )
        self.assertFalse(
            parse_exec_events(extra_tool, INBOX_POWERSHELL).child_process_admission
        )

    def test_command_enforces_profile_read_only_never_and_ephemeral(self) -> None:
        command = build_command(
            "codex.cmd", "jerry-auditor", Path("synthetic-repository"), "powershell.exe"
        )
        self.assertIn("--profile", command)
        self.assertEqual(command[command.index("--sandbox") + 1], "read-only")
        self.assertEqual(command[command.index("--ask-for-approval") + 1], "never")
        self.assertLess(command.index("--ask-for-approval"), command.index("exec"))
        self.assertIn("--ephemeral", command)
        self.assertIn("exactly once", command[-1])

    def test_preflight_classifies_child_admission_separately_from_acceptance(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = self.write_profile(root)
            path = ";".join(
                [
                    r"C:\Program Files\WindowsApps",
                    r"C:\Windows\System32\WindowsPowerShell\v1.0",
                ]
            )
            runner = FakeRunner(CommandResult(0, successful_output(), ""))
            with mock.patch("tools.dev.auditor_preflight.os.path.isfile", return_value=True):
                result = run_preflight(
                    profile="jerry-auditor",
                    profile_file=profile,
                    repo_root=root,
                    codex="codex.cmd",
                    environment={"PATH": path, "SystemRoot": r"C:\Windows"},
                    timeout=10,
                    runner=runner,
                )
            self.assertTrue(result.profile_start)
            self.assertTrue(result.child_process_admission)
            self.assertTrue(result.read_only_capability)
            self.assertTrue(result.windowsapps_path_removed)
            self.assertEqual(result.stop_reason, "NONE")
            self.assertNotIn("WindowsApps", runner.calls[0][1]["PATH"])

    def test_started_profile_with_failed_child_is_audit_admission_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = self.write_profile(root)
            runner = FakeRunner(CommandResult(0, event("thread.started"), "denied"))
            with mock.patch("tools.dev.auditor_preflight.os.path.isfile", return_value=True):
                result = run_preflight(
                    profile="jerry-auditor",
                    profile_file=profile,
                    repo_root=root,
                    codex="codex.cmd",
                    environment={
                        "PATH": r"C:\Windows",
                        "SystemRoot": r"C:\Windows",
                    },
                    timeout=10,
                    runner=runner,
                )
            self.assertTrue(result.profile_start)
            self.assertFalse(result.child_process_admission)
            self.assertFalse(result.read_only_capability)
            self.assertEqual(result.stop_reason, "AUDIT_ADMISSION_FAILED")


if __name__ == "__main__":
    unittest.main()
