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
    command_attests_harmless_child,
    is_trusted_powershell_path,
    normalize_executable_path,
    parse_exec_events,
    remove_windowsapps_from_path,
    resolve_inbox_powershell,
    resolve_trusted_powershell,
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
PS7_POWERSHELL = r"C:\Program Files\PowerShell\7\pwsh.exe"


def successful_output(powershell: str = INBOX_POWERSHELL) -> str:
    return "\n".join(
        [
            event("thread.started"),
            event(
                "item.completed",
                {
                    "type": "command_execution",
                    "command": (
                        f'"{powershell}" -NoProfile -Command '
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

    def test_preflight_writes_raw_diagnostic_log(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = self.write_profile(root)
            log_file = root / "raw_log.json"
            runner = FakeRunner(CommandResult(0, successful_output(), ""))
            with mock.patch("tools.dev.auditor_preflight.os.path.isfile", return_value=True):
                result = run_preflight(
                    profile="jerry-auditor",
                    profile_file=profile,
                    repo_root=root,
                    codex="codex.cmd",
                    environment={"PATH": r"C:\Windows", "SystemRoot": r"C:\Windows"},
                    timeout=10,
                    runner=runner,
                    raw_log=log_file,
                )
            self.assertTrue(result.read_only_capability)
            self.assertTrue(log_file.exists())
            payload = json.loads(log_file.read_text(encoding="utf-8"))
            self.assertEqual(payload["profile"], "jerry-auditor")
            self.assertEqual(payload["failure_classification"]["category"], "SUCCESS")

    def test_normalize_executable_path_handles_repeated_separators_and_slashes(self) -> None:
        raw_double = r"C:\\Program Files\\PowerShell\\7\\pwsh.exe"
        raw_quad = r"C:\\\\Program Files\\\\PowerShell\\\\7\\\\pwsh.exe"
        raw_fwd = "C:/Program Files/PowerShell/7/pwsh.exe"
        raw_mixed = r"C:\Program Files/PowerShell\\7/pwsh.exe"

        self.assertEqual(normalize_executable_path(raw_double), PS7_POWERSHELL)
        self.assertEqual(normalize_executable_path(raw_quad), PS7_POWERSHELL)
        self.assertEqual(normalize_executable_path(raw_fwd), PS7_POWERSHELL)
        self.assertEqual(normalize_executable_path(raw_mixed), PS7_POWERSHELL)

    def test_trusted_powershell_resolution_priority_and_explicit(self) -> None:
        env_both = {
            "ProgramFiles": r"C:\Program Files",
            "SystemRoot": r"C:\Windows",
            "PATH": r"C:\Program Files\PowerShell\7;C:\Windows\System32",
        }
        with mock.patch("tools.dev.auditor_preflight.os.path.isfile", return_value=True):
            # Priority 1: PS7 is preferred when available
            self.assertEqual(resolve_trusted_powershell(env_both), PS7_POWERSHELL)

            # Explicit PS7
            self.assertEqual(
                resolve_trusted_powershell(env_both, explicit=PS7_POWERSHELL),
                PS7_POWERSHELL,
            )
            # Explicit Inbox
            self.assertEqual(
                resolve_trusted_powershell(env_both, explicit=INBOX_POWERSHELL),
                INBOX_POWERSHELL,
            )
            # Explicit untrusted rejected
            self.assertIsNone(
                resolve_trusted_powershell(env_both, explicit=r"C:\Temp\pwsh.exe")
            )
            self.assertIsNone(
                resolve_trusted_powershell(env_both, explicit=r"C:\Windows\System32\cmd.exe")
            )

        # Priority 2: When PS7 is not installed, fallback to inbox
        def isfile_inbox_only(path: str) -> bool:
            return "system32" in path.lower()

        with mock.patch("tools.dev.auditor_preflight.os.path.isfile", side_effect=isfile_inbox_only):
            self.assertEqual(resolve_trusted_powershell(env_both), INBOX_POWERSHELL)

    def test_actual_diagnostic_preflight_raw_events_admitted_against_ps7(self) -> None:
        raw_events = "\n".join(
            [
                '{"type":"thread.started","thread_id":"01a1cb83-cabf-73e0-af31-315abe1ba9d1"}',
                '{"type":"turn.started"}',
                '{"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"\\"C:\\\\\\\\Program Files\\\\\\\\PowerShell\\\\\\\\7\\\\\\\\pwsh.exe\\" -NoProfile -Command \\"Write-Output \'RUNNERMESH_AUDITOR_CHILD_PROCESS_OK\'; exit 0\\"","aggregated_output":"","exit_code":null,"status":"in_progress"}}',
                '{"type":"item.completed","item":{"id":"item_0","type":"command_execution","command":"\\"C:\\\\\\\\Program Files\\\\\\\\PowerShell\\\\\\\\7\\\\\\\\pwsh.exe\\" -NoProfile -Command \\"Write-Output \'RUNNERMESH_AUDITOR_CHILD_PROCESS_OK\'; exit 0\\"","aggregated_output":"RUNNERMESH_AUDITOR_CHILD_PROCESS_OK\\r\\n","exit_code":0,"status":"completed"}}',
                '{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"AUDITOR_PREFLIGHT_COMPLETE"}}',
                '{"type":"turn.completed","usage":{"input_tokens":45413,"output_tokens":452}}',
            ]
        )
        evidence_ps7 = parse_exec_events(raw_events, PS7_POWERSHELL)
        self.assertTrue(evidence_ps7.profile_start)
        self.assertTrue(evidence_ps7.child_process_admission)
        self.assertEqual(evidence_ps7.command_count, 1)
        self.assertEqual(evidence_ps7.unexpected_completed_items, 0)

        # When bound to inbox powershell, PS7 execution must be rejected
        evidence_inbox = parse_exec_events(raw_events, INBOX_POWERSHELL)
        self.assertTrue(evidence_inbox.profile_start)
        self.assertFalse(evidence_inbox.child_process_admission)

    def test_both_explicitly_bound_powershell_paths_supported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = self.write_profile(root)
            env = {
                "ProgramFiles": r"C:\Program Files",
                "SystemRoot": r"C:\Windows",
            }
            with mock.patch("tools.dev.auditor_preflight.os.path.isfile", return_value=True):
                # Bound to PS7 and runner emits PS7
                runner_ps7 = FakeRunner(CommandResult(0, successful_output(PS7_POWERSHELL), ""))
                result_ps7 = run_preflight(
                    profile="jerry-auditor",
                    profile_file=profile,
                    repo_root=root,
                    codex="codex.cmd",
                    environment=env,
                    timeout=10,
                    runner=runner_ps7,
                    powershell=PS7_POWERSHELL,
                )
                self.assertTrue(result_ps7.read_only_capability)
                self.assertEqual(result_ps7.stop_reason, "NONE")

                # Bound to Inbox and runner emits Inbox
                runner_inbox = FakeRunner(CommandResult(0, successful_output(INBOX_POWERSHELL), ""))
                result_inbox = run_preflight(
                    profile="jerry-auditor",
                    profile_file=profile,
                    repo_root=root,
                    codex="codex.cmd",
                    environment=env,
                    timeout=10,
                    runner=runner_inbox,
                    powershell=INBOX_POWERSHELL,
                )
                self.assertTrue(result_inbox.read_only_capability)
                self.assertEqual(result_inbox.stop_reason, "NONE")

                # Mismatched binding: bound to PS7, runner emits Inbox
                runner_mismatch = FakeRunner(CommandResult(0, successful_output(INBOX_POWERSHELL), ""))
                result_mismatch = run_preflight(
                    profile="jerry-auditor",
                    profile_file=profile,
                    repo_root=root,
                    codex="codex.cmd",
                    environment=env,
                    timeout=10,
                    runner=runner_mismatch,
                    powershell=PS7_POWERSHELL,
                )
                self.assertFalse(result_mismatch.read_only_capability)
                self.assertEqual(result_mismatch.stop_reason, "AUDIT_ADMISSION_FAILED")

    def test_command_attestation_repeated_separators_and_quotes(self) -> None:
        # Repeated separators in quoted executable
        cmd_quad = (
            r'"C:\\\\Program Files\\\\PowerShell\\\\7\\\\pwsh.exe" -NoProfile -Command '
            f'"Write-Output \'{CHILD_MARKER}\'; exit 0"'
        )
        self.assertTrue(command_attests_harmless_child(cmd_quad, PS7_POWERSHELL))

        # Forward slash in quoted executable
        cmd_fwd = (
            r'"C:/Program Files/PowerShell/7/pwsh.exe" -NoProfile -Command '
            f'"Write-Output \'{CHILD_MARKER}\'; exit 0"'
        )
        self.assertTrue(command_attests_harmless_child(cmd_fwd, PS7_POWERSHELL))

        # Unquoted inbox powershell
        cmd_unquoted = (
            rf"{INBOX_POWERSHELL} -NoProfile -Command "
            f'"Write-Output \'{CHILD_MARKER}\'; exit 0"'
        )
        self.assertTrue(command_attests_harmless_child(cmd_unquoted, INBOX_POWERSHELL))

        # Double quoted command string with single quoted marker
        cmd_quotes_alt = (
            f'"{INBOX_POWERSHELL}" -NoProfile -Command '
            f"'Write-Output \"{CHILD_MARKER}\"; exit 0'"
        )
        self.assertTrue(command_attests_harmless_child(cmd_quotes_alt, INBOX_POWERSHELL))

    def test_command_attestation_rejects_additional_statements_and_flags(self) -> None:
        # Appended statement
        cmd_appended = (
            f'"{PS7_POWERSHELL}" -NoProfile -Command '
            f'"Write-Output \'{CHILD_MARKER}\'; exit 0"; whoami'
        )
        self.assertFalse(command_attests_harmless_child(cmd_appended, PS7_POWERSHELL))

        # Prepended statement
        cmd_prepended = (
            f'"{PS7_POWERSHELL}" -NoProfile -Command '
            f'"whoami; Write-Output \'{CHILD_MARKER}\'; exit 0"'
        )
        self.assertFalse(command_attests_harmless_child(cmd_prepended, PS7_POWERSHELL))

        # Chained command
        cmd_chained = (
            f'"{PS7_POWERSHELL}" -NoProfile -Command '
            f'"Write-Output \'{CHILD_MARKER}\'; exit 0" && echo pwned'
        )
        self.assertFalse(command_attests_harmless_child(cmd_chained, PS7_POWERSHELL))

        # Extra flags
        cmd_flags = (
            f'"{PS7_POWERSHELL}" -ExecutionPolicy Bypass -NoProfile -Command '
            f'"Write-Output \'{CHILD_MARKER}\'; exit 0"'
        )
        self.assertFalse(command_attests_harmless_child(cmd_flags, PS7_POWERSHELL))

        # Non-zero exit
        cmd_nonzero = (
            f'"{PS7_POWERSHELL}" -NoProfile -Command '
            f'"Write-Output \'{CHILD_MARKER}\'; exit 1"'
        )
        self.assertFalse(command_attests_harmless_child(cmd_nonzero, PS7_POWERSHELL))

    def test_command_attestation_rejects_untrusted_and_mismatched_paths(self) -> None:
        # Wrong path (temp)
        cmd_temp = (
            r'"C:\Temp\pwsh.exe" -NoProfile -Command '
            f'"Write-Output \'{CHILD_MARKER}\'; exit 0"'
        )
        self.assertFalse(command_attests_harmless_child(cmd_temp, PS7_POWERSHELL))

        # Wrong binary (cmd.exe)
        cmd_cmd = (
            r'"C:\Windows\System32\cmd.exe" -NoProfile -Command '
            f'"Write-Output \'{CHILD_MARKER}\'; exit 0"'
        )
        self.assertFalse(command_attests_harmless_child(cmd_cmd, PS7_POWERSHELL))

        # Mismatched shell: PS7 command against Inbox expected
        cmd_ps7 = (
            f'"{PS7_POWERSHELL}" -NoProfile -Command '
            f'"Write-Output \'{CHILD_MARKER}\'; exit 0"'
        )
        self.assertFalse(command_attests_harmless_child(cmd_ps7, INBOX_POWERSHELL))

    def test_event_parser_rejects_multiple_commands_and_fake_markers(self) -> None:
        # Multiple commands
        two_commands = "\n".join(
            [
                event("thread.started"),
                event(
                    "item.completed",
                    {
                        "type": "command_execution",
                        "command": f'"{PS7_POWERSHELL}" -NoProfile -Command "Write-Output \'{CHILD_MARKER}\'; exit 0"',
                        "exit_code": 0,
                        "aggregated_output": f"{CHILD_MARKER}\n",
                    },
                ),
                event(
                    "item.completed",
                    {
                        "type": "command_execution",
                        "command": f'"{PS7_POWERSHELL}" -NoProfile -Command "Write-Output \'{CHILD_MARKER}\'; exit 0"',
                        "exit_code": 0,
                        "aggregated_output": f"{CHILD_MARKER}\n",
                    },
                ),
            ]
        )
        ev_two = parse_exec_events(two_commands, PS7_POWERSHELL)
        self.assertEqual(ev_two.command_count, 2)
        self.assertFalse(ev_two.child_process_admission)

        # Output missing child marker
        missing_marker = "\n".join(
            [
                event("thread.started"),
                event(
                    "item.completed",
                    {
                        "type": "command_execution",
                        "command": f'"{PS7_POWERSHELL}" -NoProfile -Command "Write-Output \'{CHILD_MARKER}\'; exit 0"',
                        "exit_code": 0,
                        "aggregated_output": "SOME_OTHER_OUTPUT\n",
                    },
                ),
            ]
        )
        self.assertFalse(parse_exec_events(missing_marker, PS7_POWERSHELL).child_process_admission)

        # Exit code non-zero
        fail_exit = "\n".join(
            [
                event("thread.started"),
                event(
                    "item.completed",
                    {
                        "type": "command_execution",
                        "command": f'"{PS7_POWERSHELL}" -NoProfile -Command "Write-Output \'{CHILD_MARKER}\'; exit 0"',
                        "exit_code": 1,
                        "aggregated_output": f"{CHILD_MARKER}\n",
                    },
                ),
            ]
        )
        self.assertFalse(parse_exec_events(fail_exit, PS7_POWERSHELL).child_process_admission)

        # Fake marker in agent_message without command
        fake_agent_msg = "\n".join(
            [
                event("thread.started"),
                event("item.completed", {"type": "agent_message", "text": CHILD_MARKER}),
            ]
        )
        ev_fake = parse_exec_events(fake_agent_msg, PS7_POWERSHELL)
        self.assertEqual(ev_fake.command_count, 0)
        self.assertFalse(ev_fake.child_process_admission)


if __name__ == "__main__":
    unittest.main()
