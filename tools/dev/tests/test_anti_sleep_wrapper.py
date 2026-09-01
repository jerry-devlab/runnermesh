import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[3]
WRAPPER = REPO_ROOT / "tools" / "dev" / "Invoke-RunnerMeshTrain.ps1"
INHIBIT_STATE = str(0x80000000 | 0x00000001)
RESTORE_STATE = str(0x80000000)


@unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 is unavailable")
class AntiSleepWrapperTests(unittest.TestCase):
    def run_wrapper(
        self, temporary: Path, *, child_exit: int | None, missing_child: bool = False
    ) -> tuple[subprocess.CompletedProcess[str], list[str]]:
        log = temporary / "execution-state.log"
        child_prompt: str | None = None
        if missing_child:
            command = temporary / "does-not-exist"
        else:
            child = temporary / "child.py"
            child.write_text(
                "import os\nraise SystemExit(int(os.environ['DVP_CHILD_EXIT']))\n",
                encoding="utf-8",
            )
            command = Path(sys.executable)
            child_prompt = str(child)

        args = [
            "pwsh",
            "-NoProfile",
            "-File",
            str(WRAPPER),
            "-CodexCommand",
            str(command),
            "-TestExecutionStateLogPath",
            str(log),
        ]
        if child_prompt is not None:
            args.extend(["-Prompt", child_prompt])
        environment = os.environ.copy()
        if child_exit is not None:
            environment["DVP_CHILD_EXIT"] = str(child_exit)
        completed = subprocess.run(
            args,
            cwd=REPO_ROOT,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        if not log.exists():
            self.fail(
                "wrapper did not reach the execution-state seam: "
                f"stdout={completed.stdout!r} stderr={completed.stderr!r}"
            )
        states = log.read_text(encoding="utf-8").splitlines()
        return completed, states

    def test_normal_child_exit_restores_execution_state(self) -> None:
        with tempfile.TemporaryDirectory() as value:
            completed, states = self.run_wrapper(Path(value), child_exit=0)
        self.assertEqual(completed.returncode, 0)
        self.assertEqual(states, [INHIBIT_STATE, RESTORE_STATE])

    def test_nonzero_child_exit_is_preserved_and_restores(self) -> None:
        with tempfile.TemporaryDirectory() as value:
            completed, states = self.run_wrapper(Path(value), child_exit=7)
        self.assertEqual(completed.returncode, 7)
        self.assertEqual(states, [INHIBIT_STATE, RESTORE_STATE])

    def test_native_error_promotion_preserves_nonzero_exit_and_restores(self) -> None:
        with tempfile.TemporaryDirectory() as value:
            temporary = Path(value)
            child = temporary / "child.py"
            child.write_text("raise SystemExit(9)\n", encoding="utf-8")
            log = temporary / "execution-state.log"
            environment = os.environ.copy()
            environment.update(
                {
                    "DVP_WRAPPER": str(WRAPPER),
                    "DVP_CHILD": str(child),
                    "DVP_PYTHON": sys.executable,
                    "DVP_STATE_LOG": str(log),
                }
            )
            completed = subprocess.run(
                [
                    "pwsh",
                    "-NoProfile",
                    "-Command",
                    (
                        "$PSNativeCommandUseErrorActionPreference = $true; "
                        "& $env:DVP_WRAPPER -CodexCommand $env:DVP_PYTHON "
                        "-Prompt $env:DVP_CHILD "
                        "-TestExecutionStateLogPath $env:DVP_STATE_LOG; "
                        "exit $LASTEXITCODE"
                    ),
                ],
                cwd=REPO_ROOT,
                env=environment,
                check=False,
                capture_output=True,
                text=True,
                encoding="utf-8",
            )
            states = log.read_text(encoding="utf-8").splitlines()
        self.assertEqual(completed.returncode, 9)
        self.assertEqual(states, [INHIBIT_STATE, RESTORE_STATE])

    def test_wrapper_exception_restores_execution_state(self) -> None:
        with tempfile.TemporaryDirectory() as value:
            completed, states = self.run_wrapper(
                Path(value), child_exit=None, missing_child=True
            )
        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(states, [INHIBIT_STATE, RESTORE_STATE])

    def test_wrapper_never_requests_display_or_persistent_power_changes(self) -> None:
        source = WRAPPER.read_text(encoding="utf-8")
        self.assertIn("$childArguments.Add('--profile')", source)
        self.assertIn("$childArguments.Add('--model')", source)
        self.assertIn("$childArguments.Add('--')", source)
        self.assertIn("& $CodexCommand @childArguments", source)
        self.assertNotIn("CodexArguments", source)
        self.assertNotIn("--approve-for-me", source)
        self.assertNotIn("--dangerously-bypass-hook-trust", source)
        self.assertNotIn("--dangerously-bypass-approvals-and-sandbox", source)
        self.assertNotIn("--add-dir", source)
        self.assertNotIn("--enable", source)
        self.assertNotIn("Invoke-Expression", source)
        self.assertNotIn("ES_DISPLAY_REQUIRED", source)
        self.assertNotIn("powercfg", source.lower())
        self.assertNotIn("registry", source.lower())

    def test_generic_argument_forwarding_is_not_exposed(self) -> None:
        with tempfile.TemporaryDirectory() as value:
            temporary = Path(value)
            for index, argument in enumerate(
                (
                    "--yolo",
                    "--sandbox=danger-full-access",
                    "--profile=unapproved",
                    "--config=sandbox_mode=danger-full-access",
                    "-csandbox_mode=danger-full-access",
                )
            ):
                with self.subTest(argument=argument):
                    log = temporary / f"execution-state-{index}.log"
                    completed = subprocess.run(
                        [
                            "pwsh",
                            "-NoProfile",
                            "-File",
                            str(WRAPPER),
                            "-CodexCommand",
                            "where.exe",
                            "-CodexArguments",
                            argument,
                            "-TestExecutionStateLogPath",
                            str(log),
                        ],
                        cwd=REPO_ROOT,
                        check=False,
                        capture_output=True,
                        text=True,
                        encoding="utf-8",
                    )
                    self.assertNotEqual(completed.returncode, 0)
                    self.assertFalse(log.exists())


if __name__ == "__main__":
    unittest.main()
