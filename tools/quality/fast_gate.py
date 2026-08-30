#!/usr/bin/env python3
"""Run RunnerMesh's small local delta gate before pushing a settled candidate."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path
from typing import Sequence

try:
    from .change_classifier import (
        ChangeClass,
        classify_paths,
        git_changed_paths,
        resolve_base,
        resolve_head,
        risk_path_hints,
    )
    from .public_audit import audit_delta
except ImportError:  # Direct script execution.
    from change_classifier import (
        ChangeClass,
        classify_paths,
        git_changed_paths,
        resolve_base,
        resolve_head,
        risk_path_hints,
    )
    from public_audit import audit_delta


def _run(command: Sequence[str], repo_root: Path) -> None:
    print(f"RUNNING={' '.join(command)}", flush=True)
    subprocess.run(command, cwd=repo_root, check=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True, help="accepted base commit")
    parser.add_argument("--head", default="HEAD", help="settled candidate commit")
    parser.add_argument(
        "--diff-mode", choices=("merge-base", "direct"), default="merge-base"
    )
    parser.add_argument(
        "--full",
        action="store_true",
        help="add candidate-level all-target tests and Clippy",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    repo_root = Path.cwd().resolve()
    head = resolve_head(args.head, repo_root)
    base = resolve_base(args.base, head, repo_root)
    paths = git_changed_paths(base, head, args.diff_mode, repo_root)
    change_class = classify_paths(paths)

    print(f"CHANGE_CLASS={change_class.value}")
    print(f"BASE_SHA={base}")
    print(f"HEAD_SHA={head}")
    for key, value in risk_path_hints(paths).items():
        print(f"{key}={value}")
    print("RISK_PATH_HINTS=ASSISTANCE_ONLY")

    issues = audit_delta(base, head, args.diff_mode, repo_root)
    if issues:
        print("PUBLIC_AUDIT=FAIL")
        for issue in issues:
            print(issue.receipt())
        return 1
    print("PUBLIC_AUDIT=PASS")

    _run(
        [
            sys.executable,
            "-m",
            "unittest",
            "discover",
            "-s",
            "tools/quality/tests",
            "-v",
        ],
        repo_root,
    )

    if change_class is ChangeClass.DOCS_ONLY:
        print("LOCAL_CARGO_STARTED=false")
    else:
        _run(["cargo", "fmt", "--all", "--", "--check"], repo_root)
        if args.full:
            _run(["cargo", "test", "--all-targets"], repo_root)
            _run(
                [
                    "cargo",
                    "clippy",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ],
                repo_root,
            )
        else:
            _run(["cargo", "test", "--lib"], repo_root)
        print("LOCAL_CARGO_STARTED=true")

    print("ADDITIONAL_RISK_GATES=DECLARE_PER_dev_governance_files/QUALITY_GATES.md")
    print("FAST_GATE=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
