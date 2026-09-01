import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import Mapping, Sequence

from tools.dev.portability import (
    LINUX_TARGET_DIRECTORY,
    WINDOWS_TARGET_DIRECTORY,
    portability_auto_required,
    run_portability,
    wsl_clippy_command,
)
from tools.quality.change_classifier import ChangeClass


class FakeRunner:
    def __init__(self, *, available: bool, returncodes: list[int] | None = None):
        self.available = available
        self.returncodes = list(returncodes or [])
        self.commands: list[tuple[str, ...]] = []

    def which(self, command: str) -> str | None:
        return command if self.available else None

    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
        timeout: float | None = None,
    ) -> subprocess.CompletedProcess[str]:
        self.commands.append(tuple(args))
        returncode = self.returncodes.pop(0) if self.returncodes else 0
        return subprocess.CompletedProcess(args, returncode, "", "")


class LaunchFailureRunner(FakeRunner):
    def __init__(self, fail_on_call: int):
        super().__init__(available=True)
        self.fail_on_call = fail_on_call

    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
        timeout: float | None = None,
    ) -> subprocess.CompletedProcess[str]:
        self.commands.append(tuple(args))
        if len(self.commands) == self.fail_on_call:
            raise OSError("synthetic launch failure")
        return subprocess.CompletedProcess(args, 0, "", "")


class PortabilityTests(unittest.TestCase):
    def test_auto_decision_is_conservative_for_runtime_changes(self) -> None:
        self.assertTrue(
            portability_auto_required(ChangeClass.RUST_OR_RUNTIME_CHANGE)
        )
        self.assertFalse(portability_auto_required(ChangeClass.DOCS_ONLY))

    def test_docs_only_is_not_blocked_by_unavailable_wsl(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = run_portability(
                ChangeClass.DOCS_ONLY,
                "auto",
                Path(temporary),
                runner=FakeRunner(available=False),
                platform="win32",
            )
        self.assertEqual(result.status, "N/A")

    def test_unavailable_wsl_is_visible(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = run_portability(
                ChangeClass.RUST_OR_RUNTIME_CHANGE,
                "auto",
                Path(temporary),
                runner=FakeRunner(available=False),
                platform="win32",
            )
        self.assertEqual(result.status, "UNAVAILABLE")

    def test_unready_wsl_rust_is_visible(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = run_portability(
                ChangeClass.RUST_OR_RUNTIME_CHANGE,
                "auto",
                Path(temporary),
                runner=FakeRunner(available=True, returncodes=[1]),
                platform="win32",
            )
        self.assertEqual(result.status, "UNAVAILABLE")

    def test_missing_wsl_clippy_is_unavailable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = run_portability(
                ChangeClass.RUST_OR_RUNTIME_CHANGE,
                "auto",
                Path(temporary),
                runner=FakeRunner(available=True, returncodes=[1]),
                platform="win32",
            )
        self.assertEqual(result.status, "UNAVAILABLE")

    def test_wsl_readiness_launch_failure_is_unavailable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = run_portability(
                ChangeClass.RUST_OR_RUNTIME_CHANGE,
                "auto",
                Path(temporary),
                runner=LaunchFailureRunner(1),
                platform="win32",
            )
        self.assertEqual(result.status, "UNAVAILABLE")

    def test_wsl_clippy_launch_failure_is_unavailable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = run_portability(
                ChangeClass.RUST_OR_RUNTIME_CHANGE,
                "auto",
                Path(temporary),
                runner=LaunchFailureRunner(2),
                platform="win32",
            )
        self.assertEqual(result.status, "UNAVAILABLE")

    def test_windows_and_linux_targets_are_isolated(self) -> None:
        self.assertNotEqual(WINDOWS_TARGET_DIRECTORY, LINUX_TARGET_DIRECTORY)
        command = wsl_clippy_command(Path("repo"))
        self.assertEqual(command[3:5], ("sh", "-lc"))
        target_assignment = command[5].split(maxsplit=1)[0]
        self.assertEqual(
            target_assignment, f"CARGO_TARGET_DIR={LINUX_TARGET_DIRECTORY}"
        )
        self.assertNotEqual(
            target_assignment, f"CARGO_TARGET_DIR={WINDOWS_TARGET_DIRECTORY}"
        )

    def test_readiness_and_clippy_use_the_same_login_shell(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            runner = FakeRunner(available=True, returncodes=[0, 0])
            result = run_portability(
                ChangeClass.RUST_OR_RUNTIME_CHANGE,
                "auto",
                Path(temporary),
                runner=runner,
                platform="win32",
            )
        self.assertEqual(result.status, "PASS")
        self.assertEqual(runner.commands[0][3:5], ("sh", "-lc"))
        self.assertEqual(runner.commands[1][3:5], ("sh", "-lc"))
        self.assertIn("/mnt/*", runner.commands[0][5])
        self.assertIn("cargo --version", runner.commands[0][5])
        self.assertIn("cargo clippy --version", runner.commands[0][5])
        self.assertIn("host: .*linux", runner.commands[0][5])


if __name__ == "__main__":
    unittest.main()
