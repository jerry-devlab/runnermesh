#!/usr/bin/env python3
"""Conservative Linux/WSL portability checks for local Rust candidates."""

from __future__ import annotations

import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping, Protocol, Sequence

from tools.quality.change_classifier import ChangeClass


WINDOWS_TARGET_DIRECTORY = "target"
LINUX_TARGET_DIRECTORY = "target-wsl"
WSL_READINESS_SCRIPT = (
    'cargo_path="$(command -v cargo)" && '
    'rustc_path="$(command -v rustc)" && '
    'case "$cargo_path:$rustc_path" in /mnt/*|*.exe*) exit 126;; esac && '
    'cargo --version >/dev/null 2>&1 && '
    'cargo clippy --version >/dev/null 2>&1 && '
    "rustc -vV 2>/dev/null | grep -q '^host: .*linux'"
)
WSL_CLIPPY_SCRIPT = (
    f"CARGO_TARGET_DIR={LINUX_TARGET_DIRECTORY} "
    "cargo clippy --all-targets --all-features -- -D warnings"
)


@dataclass(frozen=True)
class PortabilityResult:
    status: str
    detail: str


class PortabilityRunner(Protocol):
    def which(self, command: str) -> str | None: ...

    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]: ...


class SubprocessPortabilityRunner:
    def which(self, command: str) -> str | None:
        return shutil.which(command)

    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            list(args),
            cwd=cwd,
            env=dict(env) if env is not None else None,
            check=False,
            text=True,
            encoding="utf-8",
        )


def portability_auto_required(change_class: ChangeClass) -> bool:
    return change_class is ChangeClass.RUST_OR_RUNTIME_CHANGE


def wsl_readiness_command(repo_root: Path) -> tuple[str, ...]:
    return (
        "wsl.exe",
        "--cd",
        str(repo_root),
        "sh",
        "-lc",
        WSL_READINESS_SCRIPT,
    )


def wsl_clippy_command(repo_root: Path) -> tuple[str, ...]:
    return (
        "wsl.exe",
        "--cd",
        str(repo_root),
        "sh",
        "-lc",
        WSL_CLIPPY_SCRIPT,
    )


def run_portability(
    change_class: ChangeClass,
    mode: str,
    repo_root: Path,
    *,
    runner: PortabilityRunner | None = None,
    platform: str | None = None,
) -> PortabilityResult:
    """Run an existing WSL Rust toolchain without bootstrapping or artifact sharing."""

    if mode not in {"auto", "off"}:
        raise ValueError(f"unsupported portability mode: {mode}")
    if mode == "off" or not portability_auto_required(change_class):
        return PortabilityResult("N/A", "not required for this candidate")

    effective_platform = platform or sys.platform
    if effective_platform != "win32":
        return PortabilityResult(
            "N/A", "the local full gate already runs on a non-Windows host"
        )

    active_runner = runner or SubprocessPortabilityRunner()
    if active_runner.which("wsl.exe") is None:
        return PortabilityResult("UNAVAILABLE", "wsl.exe is not available")

    try:
        ready = active_runner.run(wsl_readiness_command(repo_root), cwd=repo_root)
    except OSError:
        return PortabilityResult("UNAVAILABLE", "wsl.exe could not start")
    if ready.returncode != 0:
        return PortabilityResult(
            "UNAVAILABLE", "an existing WSL cargo/rustc environment was not ready"
        )

    try:
        completed = active_runner.run(wsl_clippy_command(repo_root), cwd=repo_root)
    except OSError:
        return PortabilityResult("UNAVAILABLE", "wsl.exe could not start")
    if completed.returncode != 0:
        return PortabilityResult("FAIL", "WSL cargo clippy failed")
    return PortabilityResult("PASS", "WSL cargo clippy passed")
