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
PS7_POWERSHELL_NAME = "pwsh.exe"
INBOX_POWERSHELL_NAME = "powershell.exe"


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
            stdin=subprocess.DEVNULL,
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


def normalize_executable_path(path_str: str) -> str:
    """Normalize repeated path separators in an executable path."""
    return re.sub(r"[\\/]+", r"\\", path_str).strip()


def is_trusted_powershell_path(candidate: str | Path, environment: Mapping[str, str]) -> bool:
    norm = normalize_executable_path(str(candidate))
    pure = PureWindowsPath(norm)
    name = pure.name.lower()
    if name not in (PS7_POWERSHELL_NAME, INBOX_POWERSHELL_NAME):
        return False
    if not os.path.isfile(norm):
        return False

    norm_lower = norm.lower()
    for forbidden in ("windowsapps", "temp", "appdata", "users", "desktop"):
        if forbidden in norm_lower:
            return False

    standard_program_files = [r"c:\program files", r"c:\program files (x86)"]
    if "ProgramFiles" in environment and environment["ProgramFiles"]:
        pf = environment["ProgramFiles"].rstrip(r"\/").lower()
        if pf not in standard_program_files:
            standard_program_files.append(pf)
    if "ProgramW6432" in environment and environment["ProgramW6432"]:
        pw = environment["ProgramW6432"].rstrip(r"\/").lower()
        if pw not in standard_program_files:
            standard_program_files.append(pw)

    standard_system_roots = [r"c:\windows"]
    if "SystemRoot" in environment and environment["SystemRoot"]:
        sr = environment["SystemRoot"].rstrip(r"\/").lower()
        if sr not in standard_system_roots:
            standard_system_roots.append(sr)
    if "WINDIR" in environment and environment["WINDIR"]:
        wd = environment["WINDIR"].rstrip(r"\/").lower()
        if wd not in standard_system_roots:
            standard_system_roots.append(wd)

    if name == PS7_POWERSHELL_NAME:
        valid_ps7_roots = [
            f"{pf}\\powershell\\7\\pwsh.exe" for pf in standard_program_files
        ]
        return any(norm_lower == root for root in valid_ps7_roots)

    if name == INBOX_POWERSHELL_NAME:
        valid_inbox_roots = []
        for sr in standard_system_roots:
            valid_inbox_roots.append(f"{sr}\\system32\\windowspowershell\\v1.0\\powershell.exe")
            valid_inbox_roots.append(f"{sr}\\syswow64\\windowspowershell\\v1.0\\powershell.exe")
        return any(norm_lower == root for root in valid_inbox_roots)

    return False


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
        / INBOX_POWERSHELL_NAME
    )
    return candidate if is_trusted_powershell_path(candidate, environment) else None


def resolve_trusted_powershell(
    environment: Mapping[str, str], explicit: str | None = None
) -> str | None:
    """Resolve an explicit or standard trusted PowerShell executable."""
    if explicit:
        norm = normalize_executable_path(explicit)
        return norm if is_trusted_powershell_path(norm, environment) else None

    # Priority 1: PowerShell 7 if installed and trusted
    path_pwsh = shutil.which(PS7_POWERSHELL_NAME, path=environment.get("PATH"))
    if path_pwsh and is_trusted_powershell_path(path_pwsh, environment):
        return normalize_executable_path(path_pwsh)

    program_files = environment.get("ProgramFiles") or environment.get("ProgramW6432")
    if program_files:
        pwsh_candidate = str(PureWindowsPath(program_files) / "PowerShell" / "7" / PS7_POWERSHELL_NAME)
        if is_trusted_powershell_path(pwsh_candidate, environment):
            return pwsh_candidate

    # Priority 2: Windows Inbox PowerShell 5.1
    return resolve_inbox_powershell(environment)


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


HARMLSS_ARGS_PATTERN = re.compile(
    rf"^(?:-NoProfile\s+)?-Command\s+(?:"
    rf'"Write-Output\s+\'{re.escape(CHILD_MARKER)}\'\s*;\s*exit\s+0"|'
    rf"'Write-Output\s+\"{re.escape(CHILD_MARKER)}\"\s*;\s*exit\s+0'"
    rf")$",
    re.IGNORECASE,
)


def command_attests_harmless_child(command: object, powershell: str) -> bool:
    if not isinstance(command, str):
        return False
    stripped = command.strip()
    if stripped.count(CHILD_MARKER) != 1:
        return False

    if stripped.startswith('"'):
        end_quote = stripped.find('"', 1)
        if end_quote == -1:
            return False
        raw_exe = stripped[1:end_quote]
        rest = stripped[end_quote + 1:].strip()
    else:
        parts = stripped.split(None, 1)
        if len(parts) != 2:
            return False
        raw_exe = parts[0]
        rest = parts[1].strip()

    normalized_exe = normalize_executable_path(raw_exe)
    expected_exe = normalize_executable_path(powershell)
    if PureWindowsPath(normalized_exe) != PureWindowsPath(expected_exe):
        return False

    return HARMLSS_ARGS_PATTERN.fullmatch(rest) is not None


def parse_exec_events(output: str, powershell: str) -> AdmissionEvidence:
    profile_start = False
    commands_started = 0
    commands_completed = 0
    admitted = False
    unexpected_completed_items = 0
    non_json_lines = 0

    for line in output.splitlines():
        trimmed = line.strip()
        if not trimmed:
            continue
        try:
            event = json.loads(trimmed)
        except json.JSONDecodeError:
            non_json_lines += 1
            continue

        event_type = event.get("type")
        if event_type == "thread.started":
            profile_start = True
            continue

        if event_type == "item.started":
            item = event.get("item")
            if isinstance(item, dict) and item.get("type") == "command_execution":
                commands_started += 1
                cmd = item.get("command")
                if not command_attests_harmless_child(cmd, powershell):
                    unexpected_completed_items += 1
            continue

        if event_type == "item.completed":
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

            commands_completed += 1
            output_text = str(item.get("aggregated_output") or item.get("output") or "")
            if (
                item.get("exit_code") == 0
                and command_attests_harmless_child(item.get("command"), powershell)
                and CHILD_MARKER in output_text
            ):
                admitted = True
            else:
                unexpected_completed_items += 1
            continue

        if event_type in ("turn.started", "turn.completed"):
            continue

    commands_consistent = commands_started == 0 or commands_started == commands_completed
    child_admitted = (
        profile_start
        and admitted
        and commands_completed == 1
        and commands_consistent
        and unexpected_completed_items == 0
        and non_json_lines == 0
    )
    return AdmissionEvidence(
        profile_start,
        child_admitted,
        commands_completed,
        unexpected_completed_items + non_json_lines,
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


def classify_failure(
    *,
    completed: CommandResult | None,
    timeout_occurred: bool,
    os_error_occurred: bool,
    evidence: AdmissionEvidence | None,
) -> tuple[str, str]:
    if timeout_occurred:
        return "TIMEOUT", "Process execution exceeded timeout threshold"
    if os_error_occurred or completed is None:
        return "STARTUP_FAILURE", "Failed to launch Codex process (OSError)"
    if not evidence or not evidence.profile_start:
        return "STARTUP_FAILURE", "Codex process started but thread.started event was not emitted"
    if completed.returncode != 0:
        return "COMMAND_EXECUTION_FAILURE", f"Codex process exited with non-zero code {completed.returncode}"
    if evidence.command_count == 0:
        return "COMMAND_EXECUTION_FAILURE", "No command_execution event emitted by Codex in read-only sandbox"
    if evidence.unexpected_completed_items > 0:
        return "EVIDENCE_PARSING_FAILURE", f"Unexpected completed items count ({evidence.unexpected_completed_items})"
    if not evidence.child_process_admission:
        return "EVIDENCE_PARSING_FAILURE", "Command execution output did not match child marker or shell resolution contract"
    return "SUCCESS", "All admission checks passed"


def run_preflight(
    *,
    profile: str,
    profile_file: Path,
    repo_root: Path,
    codex: str,
    environment: Mapping[str, str],
    timeout: float,
    runner: Runner,
    raw_log: Path | None = None,
    powershell: str | None = None,
) -> PreflightResult:
    try:
        validate_profile_contract(profile_file)
    except ValueError:
        return PreflightResult(False, False, False, False, "PROFILE_CONTRACT_INVALID")

    child_env = dict(environment)
    child_env["PATH"], removed = remove_windowsapps_from_path(
        child_env.get("PATH", "")
    )
    resolved_powershell = resolve_trusted_powershell(child_env, explicit=powershell)
    if not resolved_powershell:
        return PreflightResult(False, False, False, removed, "SHELL_UNAVAILABLE")

    command = build_command(codex, profile, repo_root, resolved_powershell)
    completed: CommandResult | None = None
    timeout_occurred = False
    os_error_occurred = False
    evidence: AdmissionEvidence | None = None
    preflight_result: PreflightResult

    try:
        completed = runner.run(command, cwd=repo_root, env=child_env, timeout=timeout)
    except subprocess.TimeoutExpired:
        timeout_occurred = True
        preflight_result = PreflightResult(False, False, False, removed, "AUDIT_ADMISSION_TIMEOUT")
    except OSError:
        os_error_occurred = True
        preflight_result = PreflightResult(False, False, False, removed, "PROFILE_START_FAILED")
    else:
        evidence = parse_exec_events(completed.stdout, resolved_powershell)
        child_pass = evidence.child_process_admission and completed.returncode == 0
        if child_pass:
            preflight_result = PreflightResult(evidence.profile_start, True, True, removed, "NONE")
        elif evidence.profile_start:
            preflight_result = PreflightResult(True, False, False, removed, "AUDIT_ADMISSION_FAILED")
        else:
            preflight_result = PreflightResult(False, False, False, removed, "PROFILE_START_FAILED")

    if raw_log:
        failure_cat, failure_det = classify_failure(
            completed=completed,
            timeout_occurred=timeout_occurred,
            os_error_occurred=os_error_occurred,
            evidence=evidence,
        )
        raw_events: list[dict[str, object]] = []
        if completed and completed.stdout:
            for line in completed.stdout.splitlines():
                try:
                    raw_events.append(json.loads(line))
                except json.JSONDecodeError:
                    pass
        log_payload = {
            "profile": profile,
            "codex": codex,
            "repo_root": str(repo_root),
            "powershell": resolved_powershell,
            "command": list(command),
            "timeout_seconds": timeout,
            "exit_code": completed.returncode if completed else None,
            "stdout_raw": completed.stdout if completed else "",
            "stderr_raw": completed.stderr if completed else "",
            "events_count": len(raw_events),
            "events": raw_events,
            "evidence": {
                "profile_start": evidence.profile_start,
                "child_process_admission": evidence.child_process_admission,
                "command_count": evidence.command_count,
                "unexpected_completed_items": evidence.unexpected_completed_items,
            } if evidence else None,
            "failure_classification": {
                "category": failure_cat,
                "detail": failure_det,
            },
            "stop_reason": preflight_result.stop_reason,
            "read_only_capability_pass": preflight_result.read_only_capability,
        }
        raw_log.parent.mkdir(parents=True, exist_ok=True)
        raw_log.write_text(json.dumps(log_payload, indent=2), encoding="utf-8")

    return preflight_result


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=ALLOWED_PROFILES, default="jerry-auditor")
    parser.add_argument("--codex", help="explicit Codex CLI executable")
    parser.add_argument("--powershell", help="explicit trusted PowerShell executable path")
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    parser.add_argument("--timeout", type=float, default=300.0)
    parser.add_argument("--raw-log", type=Path, help="write raw diagnostic logs as JSON")
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
        raw_log=args.raw_log,
        powershell=args.powershell,
    )
    result.emit(args.profile)
    return 0 if result.read_only_capability else 1


if __name__ == "__main__":
    raise SystemExit(main())
