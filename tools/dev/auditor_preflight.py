#!/usr/bin/env python3
"""Prove that a read-only Codex review profile can start one harmless child."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path, PureWindowsPath
from typing import Mapping, Protocol, Sequence


REPO_ROOT = Path(__file__).resolve().parents[2]
ALLOWED_PROFILES = ("jerry-auditor", "jerry-supervisor")
CHILD_MARKER = "RUNNERMESH_AUDITOR_CHILD_PROCESS_OK"


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str = ""
    stderr: str = ""


class Runner(Protocol):
    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str],
        timeout: float,
    ) -> CommandResult: ...


class SubprocessRunner:
    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str],
        timeout: float,
    ) -> CommandResult:
        completed = subprocess.run(
            list(args),
            cwd=cwd,
            env=dict(env),
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=timeout,
        )
        return CommandResult(completed.returncode, completed.stdout, completed.stderr)


@dataclass(frozen=True)
class AdmissionEvidence:
    profile_start: bool
    child_process_admission: bool
    command_count: int
    unexpected_completed_items: int


@dataclass(frozen=True)
class PreflightResult:
    profile_start: bool
    child_process_admission: bool
    read_only_capability: bool
    windowsapps_path_removed: bool
    stop_reason: str

    def emit(self, profile: str) -> None:
        fields = {
            "PROFILE": profile,
            "PROFILE_START_PASS": self.profile_start,
            "CHILD_PROCESS_ADMISSION_PASS": self.child_process_admission,
            "AUDITOR_READ_ONLY_CAPABILITY_PASS": self.read_only_capability,
            "AUDIT_ACCEPTANCE_PASS": False,
            "EVIDENCE_SCOPE": "ADMISSION_ONLY",
            "WINDOWSAPPS_PATH_REMOVED": self.windowsapps_path_removed,
            "STOP_REASON": self.stop_reason,
        }
        for key, value in fields.items():
            rendered = str(value).lower() if isinstance(value, bool) else value
            print(f"{key}={rendered}")


def remove_windowsapps_from_path(value: str) -> tuple[str, bool]:
    """Remove packaged executable resolution from one child environment only."""

    kept: list[str] = []
    removed = False
    for entry in value.split(";"):
        normalized = entry.rstrip("\\/").replace("/", "\\").lower()
        if "\\windowsapps" in normalized:
            removed = True
        else:
            kept.append(entry)
    return ";".join(kept), removed


def resolve_inbox_powershell(environment: Mapping[str, str]) -> str | None:
    """Resolve only the Windows inbox PowerShell executable."""

    system_root = environment.get("SystemRoot") or environment.get("WINDIR")
    if not system_root:
        return None
    candidate = str(
        PureWindowsPath(system_root)
        / "System32"
        / "WindowsPowerShell"
        / "v1.0"
        / "powershell.exe"
    )
    return candidate if os.path.isfile(candidate) else None


def validate_profile_contract(profile_file: Path) -> None:
    try:
        with profile_file.open("rb") as stream:
            config = tomllib.load(stream)
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ValueError("profile configuration is unavailable or invalid") from error

    if config.get("sandbox_mode") != "read-only":
        raise ValueError("profile sandbox_mode must be read-only")
    if config.get("approval_policy") != "never":
        raise ValueError("profile approval_policy must be never")


def command_attests_harmless_child(command: object, powershell: str) -> bool:
    if not isinstance(command, str):
        return False
    rendered = command.strip().replace("/", "\\")
    expected_shell = re.escape(str(PureWindowsPath(powershell)))
    if rendered.count(CHILD_MARKER) != 1:
        return False
    harmless_command = re.compile(
        rf'^(?:"{expected_shell}"|{expected_shell})\s+'
        rf'-NoProfile\s+-Command\s+(?:'
        rf'"Write-Output\s+\'{re.escape(CHILD_MARKER)}\'\s*;\s*exit\s+0"|'
        rf"'Write-Output\s+\"{re.escape(CHILD_MARKER)}\"\s*;\s*exit\s+0'"
        rf')$',
        re.IGNORECASE,
    )
    return harmless_command.fullmatch(rendered) is not None


def parse_exec_events(output: str, powershell: str) -> AdmissionEvidence:
    profile_start = False
    command_count = 0
    admitted = False
    unexpected_completed_items = 0
    for line in output.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") == "thread.started":
            profile_start = True
        if event.get("type") != "item.completed":
            continue
        item = event.get("item")
        if not isinstance(item, dict):
            unexpected_completed_items += 1
            continue
        item_type = item.get("type")
        if item_type in ("agent_message", "reasoning"):
            continue
        if item_type != "command_execution":
            unexpected_completed_items += 1
            continue
        command_count += 1
        if (
            item.get("exit_code") == 0
            and command_attests_harmless_child(item.get("command"), powershell)
            and CHILD_MARKER in str(item.get("aggregated_output") or "")
        ):
            admitted = True
    child_admitted = (
        profile_start
        and admitted
        and command_count == 1
        and unexpected_completed_items == 0
    )
    return AdmissionEvidence(
        profile_start,
        child_admitted,
        command_count,
        unexpected_completed_items,
    )


def build_command(
    codex: str, profile: str, repo_root: Path, powershell: str
) -> tuple[str, ...]:
    child_command = f"Write-Output '{CHILD_MARKER}'; exit 0"
    prompt = (
        "This is an admission preflight, not an audit. Use the shell tool exactly once. "
        f"The admitted shell must resolve to '{powershell}'. Run this literal PowerShell "
        f"source and nothing else: {child_command} "
        "Do not inspect repository files, write files, use network tools, or call any "
        "other tool. After the command, answer exactly AUDITOR_PREFLIGHT_COMPLETE."
    )
    return (
        codex,
        "--profile",
        profile,
        "--sandbox",
        "read-only",
        "--ask-for-approval",
        "never",
        "exec",
        "--ephemeral",
        "--json",
        "--color",
        "never",
        "--cd",
        str(repo_root),
        prompt,
    )


def resolve_codex(explicit: str | None) -> str | None:
    if explicit:
        return explicit
    return shutil.which("codex.cmd") or shutil.which("codex.exe") or shutil.which("codex")


def run_preflight(
    *,
    profile: str,
    profile_file: Path,
    repo_root: Path,
    codex: str,
    environment: Mapping[str, str],
    timeout: float,
    runner: Runner,
) -> PreflightResult:
    try:
        validate_profile_contract(profile_file)
    except ValueError:
        return PreflightResult(False, False, False, False, "PROFILE_CONTRACT_INVALID")

    child_env = dict(environment)
    child_env["PATH"], removed = remove_windowsapps_from_path(
        child_env.get("PATH", "")
    )
    powershell = resolve_inbox_powershell(child_env)
    if not powershell:
        return PreflightResult(False, False, False, removed, "SHELL_UNAVAILABLE")

    command = build_command(codex, profile, repo_root, powershell)
    try:
        completed = runner.run(command, cwd=repo_root, env=child_env, timeout=timeout)
    except subprocess.TimeoutExpired:
        return PreflightResult(False, False, False, removed, "AUDIT_ADMISSION_TIMEOUT")
    except OSError:
        return PreflightResult(False, False, False, removed, "PROFILE_START_FAILED")

    evidence = parse_exec_events(completed.stdout, powershell)
    child_pass = evidence.child_process_admission and completed.returncode == 0
    if child_pass:
        return PreflightResult(evidence.profile_start, True, True, removed, "NONE")
    if evidence.profile_start:
        return PreflightResult(True, False, False, removed, "AUDIT_ADMISSION_FAILED")
    return PreflightResult(False, False, False, removed, "PROFILE_START_FAILED")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=ALLOWED_PROFILES, default="jerry-auditor")
    parser.add_argument("--codex", help="explicit Codex CLI executable")
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    parser.add_argument("--timeout", type=float, default=300.0)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if sys.platform != "win32":
        result = PreflightResult(False, False, False, False, "PLATFORM_UNSUPPORTED")
        result.emit(args.profile)
        return 1

    codex = resolve_codex(args.codex)
    if not codex:
        result = PreflightResult(False, False, False, False, "CODEX_UNAVAILABLE")
        result.emit(args.profile)
        return 1

    codex_home = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex"))
    profile_file = codex_home / f"{args.profile}.config.toml"
    result = run_preflight(
        profile=args.profile,
        profile_file=profile_file,
        repo_root=args.repo_root.resolve(),
        codex=codex,
        environment=os.environ,
        timeout=args.timeout,
        runner=SubprocessRunner(),
    )
    result.emit(args.profile)
    return 0 if result.read_only_capability else 1


if __name__ == "__main__":
    raise SystemExit(main())
