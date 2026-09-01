#!/usr/bin/env python3
"""Conservative path-based change classification for RunnerMesh CI.

The classifier decides only whether a delta is safe for the documentation fast
path.  Its risk-path hints are review aids, not semantic evidence or permission
to reuse an accepted gate.
"""

from __future__ import annotations

import argparse
import enum
import json
import subprocess
from pathlib import Path, PurePosixPath
from typing import Iterable, Sequence


class ChangeClass(str, enum.Enum):
    DOCS_ONLY = "DOCS_ONLY"
    RUST_OR_RUNTIME_CHANGE = "RUST_OR_RUNTIME_CHANGE"


RUNTIME_PREFIXES = (
    ".github/actions/",
    ".github/workflows/",
    "resources/",
    "scripts/",
    "src/",
    "tests/",
    "tools/dev/",
    "tools/quality/",
)
RUNTIME_BASENAMES = {"cargo.lock", "cargo.toml", "build.rs"}
DOCS_ROOTS = ("dev_governance_files/", "docs/", "goals/")
DOCS_SUFFIXES = {".md", ".rst", ".txt"}

# These are deliberately narrow hints.  An empty path hint is never sufficient
# by itself to declare a semantic risk diff empty.
RISK_PATH_MARKERS = {
    "TRAY_PRESENTATION": (
        "src/tray.rs",
        "src/windows_tray_theme.rs",
        "resources/runnermesh-agent.",
    ),
    "PROBE_POLICY": (
        "src/policy.rs",
        "src/probe.rs",
        "docs/resource-policy.md",
    ),
    "RUNNER_CONTROL": (
        "src/admission.rs",
        "src/host.rs",
        "src/qualification.rs",
        "src/runner_observer.rs",
        "src/supervisor.rs",
        "src/windows_supervisor.rs",
    ),
    "PERSISTENT_CONFIG_SAFETY": (
        "autostart",
        "config",
        "preferences",
        "registry",
    ),
    "INSTALL_ACTIVATION_SAFETY": (
        "install",
        "package",
        "release",
        "rollback",
        "update",
    ),
    "SECURITY_PRIVACY": (
        ".github/",
        "security.md",
        "threat-model",
        "tools/dev/invoke-runnermeshtrain.ps1",
        "tools/dev/train.py",
        "tools/quality/public_audit.py",
    ),
    "RELEASE_GATE": (
        "release",
        "cargo.lock",
        "cargo.toml",
    ),
}


def normalize_path(path: str) -> str:
    normalized = path.strip().replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    return str(PurePosixPath(normalized)).lower()


def is_runtime_path(path: str) -> bool:
    normalized = normalize_path(path)
    if normalized in RUNTIME_BASENAMES:
        return True
    return normalized.startswith(RUNTIME_PREFIXES)


def is_docs_path(path: str) -> bool:
    normalized = normalize_path(path)
    if is_runtime_path(normalized):
        return False
    suffix = PurePosixPath(normalized).suffix
    if suffix == ".md":
        return True
    return normalized.startswith(DOCS_ROOTS) and suffix in DOCS_SUFFIXES


def classify_paths(paths: Iterable[str]) -> ChangeClass:
    normalized = sorted({normalize_path(path) for path in paths if path.strip()})
    if not normalized:
        return ChangeClass.RUST_OR_RUNTIME_CHANGE
    if all(is_docs_path(path) for path in normalized):
        return ChangeClass.DOCS_ONLY
    return ChangeClass.RUST_OR_RUNTIME_CHANGE


def risk_path_hints(paths: Iterable[str]) -> dict[str, str]:
    normalized = tuple(normalize_path(path) for path in paths if path.strip())
    hints: dict[str, str] = {}
    for gate, markers in RISK_PATH_MARKERS.items():
        changed = any(marker in path for path in normalized for marker in markers)
        hints[f"{gate}_PATH_DIFF"] = "CHANGED" if changed else "EMPTY"
    return hints


def _run_git(args: Sequence[str], repo_root: Path) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return completed.stdout.strip()


def resolve_base(base: str | None, head: str, repo_root: Path) -> str:
    if base and base.strip("0"):
        return _run_git(["rev-parse", "--verify", f"{base}^{{commit}}"], repo_root)
    return _run_git(["rev-parse", "--verify", f"{head}^"], repo_root)


def resolve_head(head: str, repo_root: Path) -> str:
    return _run_git(["rev-parse", "--verify", f"{head}^{{commit}}"], repo_root)


def diff_spec(base: str, head: str, diff_mode: str) -> str:
    separator = "..." if diff_mode == "merge-base" else ".."
    return f"{base}{separator}{head}"


def git_changed_paths(
    base: str, head: str, diff_mode: str, repo_root: Path
) -> list[str]:
    output = _run_git(
        [
            "diff",
            "--no-renames",
            "--name-only",
            "--diff-filter=ACDMRTUXB",
            diff_spec(base, head, diff_mode),
        ],
        repo_root,
    )
    return [line for line in output.splitlines() if line]


def result_fields(
    paths: Iterable[str],
    base: str,
    head: str,
    diff_mode: str,
    event_name: str | None = None,
) -> dict[str, str]:
    path_list = sorted({normalize_path(path) for path in paths if path.strip()})
    fields = {
        "change_class": classify_paths(path_list).value,
        "changed_path_count": str(len(path_list)),
        "base_sha": base,
        "head_sha": head,
        "diff_mode": diff_mode,
        "path_hints_are_semantic_proof": "false",
    }
    if event_name is not None:
        try:
            from .ci_policy import decide_ci_jobs
        except ImportError:  # Direct script execution.
            from ci_policy import decide_ci_jobs

        decision = decide_ci_jobs(event_name, classify_paths(path_list))
        fields["code_ci_required"] = str(decision.code_ci_required).lower()
    fields.update({key.lower(): value for key, value in risk_path_hints(path_list).items()})
    return fields


def _write_github_output(path: Path, fields: dict[str, str]) -> None:
    with path.open("a", encoding="utf-8", newline="\n") as handle:
        for key, value in fields.items():
            handle.write(f"{key}={value}\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="base commit; defaults to HEAD parent")
    parser.add_argument("--head", default="HEAD", help="candidate commit")
    parser.add_argument(
        "--diff-mode", choices=("merge-base", "direct"), default="merge-base"
    )
    parser.add_argument(
        "--event-name",
        choices=("pull_request", "push", "workflow_dispatch"),
        help="emit the event-aware code_ci_required workflow decision",
    )
    parser.add_argument(
        "--path",
        action="append",
        dest="paths",
        help="classify an explicit path instead of reading a Git diff",
    )
    parser.add_argument("--json", action="store_true", help="emit JSON")
    parser.add_argument("--github-output", type=Path, help="append job outputs")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    repo_root = Path.cwd()
    head = resolve_head(args.head, repo_root)
    base = resolve_base(args.base, head, repo_root)
    paths = args.paths or git_changed_paths(base, head, args.diff_mode, repo_root)
    fields = result_fields(paths, base, head, args.diff_mode, args.event_name)

    if args.github_output:
        _write_github_output(args.github_output, fields)
    if args.json:
        print(json.dumps({**fields, "changed_paths": sorted(paths)}, indent=2))
    else:
        for key, value in fields.items():
            print(f"{key.upper()}={value}")
        print("RISK_PATH_HINTS=ASSISTANCE_ONLY")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
