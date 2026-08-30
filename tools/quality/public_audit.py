#!/usr/bin/env python3
"""Deterministic baseline audit for content entering the public repository.

This scanner checks machine-detectable mistakes.  It does not replace review of
security, privacy, trust, or product semantics.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence
from urllib.parse import unquote

try:
    from .change_classifier import diff_spec, git_changed_paths, resolve_base, resolve_head
except ImportError:  # Direct script execution.
    from change_classifier import diff_spec, git_changed_paths, resolve_base, resolve_head


@dataclass(frozen=True)
class AuditIssue:
    path: str
    line: int | None
    reason: str

    def receipt(self) -> str:
        location = self.path if self.line is None else f"{self.path}:{self.line}"
        return f"{location}: {self.reason}"


@dataclass(frozen=True)
class SensitiveRule:
    reason: str
    pattern: re.Pattern[str]


SENSITIVE_RULES = (
    SensitiveRule(
        "personal Windows user path",
        re.compile(r"(?i)\b[A-Z]:[\\/]+Users[\\/]+[^\\/\s`'\"()]+"),
    ),
    SensitiveRule(
        "personal Unix or macOS home path",
        re.compile(
            r"(?i)(?<![A-Za-z0-9_])(?:/"
            r"home/|/"
            r"Users/)[^/\s`'\"()]+"
        ),
    ),
    SensitiveRule(
        "private source or workstation root",
        re.compile(r"(?i)\b[A-Z]:[\\/]+(?:src|dev)[\\/]+[^\s`'\"()]+"),
    ),
    SensitiveRule(
        "private runner-home shaped path",
        re.compile(r"(?i)\b[A-Z]:[\\/]+[^\s`'\"()]*actions[-_]runner[^\s`'\"()]*"),
    ),
    SensitiveRule(
        "private Codex evidence root",
        re.compile(r"(?i)\.codex[\\/]+(?:sessions|memories)(?:[\\/]|\b)"),
    ),
    SensitiveRule(
        "personal runner service identity",
        re.compile(r"(?i)\bactions\.runner\.[A-Za-z0-9_.-]+\.[A-Za-z0-9_.-]+\b"),
    ),
    SensitiveRule(
        "workstation hostname shape",
        re.compile(r"(?i)\b(?:desktop|laptop|win)-[A-Z0-9]{5,}\b"),
    ),
    SensitiveRule(
        "GitHub token shape",
        re.compile(r"\bgh(?:p|o|u|s|r)_[A-Za-z0-9]{20,}\b"),
    ),
    SensitiveRule(
        "OpenAI API key shape",
        re.compile(r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b"),
    ),
    SensitiveRule("AWS access key shape", re.compile(r"\bAKIA[A-Z0-9]{16}\b")),
    SensitiveRule(
        "private key material header",
        re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    ),
)

MARKDOWN_LINK = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
HUNK_HEADER = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")


def _looks_like_placeholder(value: str) -> bool:
    lowered = value.lower()
    return any(
        marker in lowered
        for marker in ("<user", "<name", "{user", "${user", "%user", "example")
    )


def scan_sensitive_text(text: str) -> list[str]:
    reasons: list[str] = []
    for rule in SENSITIVE_RULES:
        for match in rule.pattern.finditer(text):
            if not _looks_like_placeholder(match.group(0)):
                reasons.append(rule.reason)
                break
    return reasons


def _run_git(
    args: Sequence[str], repo_root: Path, check: bool = True
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=repo_root,
        check=check,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )


def collect_added_lines(
    base: str, head: str, diff_mode: str, path: str, repo_root: Path
) -> list[tuple[int, str]]:
    completed = _run_git(
        [
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--unified=0",
            diff_spec(base, head, diff_mode),
            "--",
            path,
        ],
        repo_root,
    )
    added: list[tuple[int, str]] = []
    new_line: int | None = None
    for line in completed.stdout.splitlines():
        header = HUNK_HEADER.match(line)
        if header:
            new_line = int(header.group(1))
            continue
        if new_line is None or line.startswith("+++"):
            continue
        if line.startswith("+"):
            added.append((new_line, line[1:]))
            new_line += 1
        elif line.startswith("-"):
            continue
        elif not line.startswith("\\"):
            new_line += 1
    return added


def _relative_link_target(raw_target: str) -> str | None:
    target = raw_target.strip()
    if target.startswith("<") and ">" in target:
        target = target[1 : target.index(">")]
    else:
        target = target.split(maxsplit=1)[0]
    target = unquote(target.split("#", 1)[0].split("?", 1)[0]).strip()
    if not target or target.startswith("#"):
        return None
    if re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:", target) or target.startswith("//"):
        return None
    if re.match(r"^[A-Za-z]:[\\/]", target) or target.startswith(("/", "\\")):
        return None
    return target


def check_markdown_links(markdown_path: Path, repo_root: Path) -> list[AuditIssue]:
    issues: list[AuditIssue] = []
    try:
        content = markdown_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return [
            AuditIssue(
                markdown_path.relative_to(repo_root).as_posix(),
                None,
                "Markdown file is not readable UTF-8",
            )
        ]
    relative_path = markdown_path.relative_to(repo_root).as_posix()
    for line_number, line in enumerate(content.splitlines(), start=1):
        for match in MARKDOWN_LINK.finditer(line):
            target = _relative_link_target(match.group(1))
            if target is None:
                continue
            resolved = (markdown_path.parent / target).resolve()
            try:
                resolved.relative_to(repo_root.resolve())
            except ValueError:
                issues.append(
                    AuditIssue(relative_path, line_number, "relative Markdown link escapes repository")
                )
                continue
            if not resolved.exists():
                issues.append(
                    AuditIssue(relative_path, line_number, "relative Markdown link target is missing")
                )
    return issues


def audit_delta(
    base: str, head: str, diff_mode: str, repo_root: Path
) -> list[AuditIssue]:
    issues: list[AuditIssue] = []
    spec = diff_spec(base, head, diff_mode)
    whitespace = _run_git(["diff", "--check", spec], repo_root, check=False)
    if whitespace.returncode != 0:
        issues.append(AuditIssue("<diff>", None, "Git diff whitespace check failed"))

    paths = git_changed_paths(base, head, diff_mode, repo_root)
    for path in paths:
        for reason in scan_sensitive_text(path):
            issues.append(AuditIssue(path, None, reason))
        for line_number, line in collect_added_lines(base, head, diff_mode, path, repo_root):
            for reason in scan_sensitive_text(line):
                issues.append(AuditIssue(path, line_number, reason))
        candidate = repo_root / path
    # Validate all tracked Markdown so a deleted target cannot break a link in
    # an otherwise unchanged document.
    tracked_markdown = _run_git(["ls-files", "*.md"], repo_root).stdout.splitlines()
    for markdown in tracked_markdown:
        candidate = repo_root / markdown
        if candidate.exists():
            issues.extend(check_markdown_links(candidate, repo_root))
    return issues


def audit_repository(repo_root: Path) -> list[AuditIssue]:
    issues: list[AuditIssue] = []
    tracked = _run_git(["ls-files", "-z"], repo_root).stdout.split("\0")
    for path in (item for item in tracked if item):
        candidate = repo_root / path
        for reason in scan_sensitive_text(path):
            issues.append(AuditIssue(path, None, reason))
        try:
            content = candidate.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for line_number, line in enumerate(content.splitlines(), start=1):
            for reason in scan_sensitive_text(line):
                issues.append(AuditIssue(path, line_number, reason))
        if candidate.suffix.lower() == ".md":
            issues.extend(check_markdown_links(candidate, repo_root))
    return issues


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="base commit; defaults to HEAD parent")
    parser.add_argument("--head", default="HEAD", help="candidate commit")
    parser.add_argument(
        "--diff-mode", choices=("merge-base", "direct"), default="merge-base"
    )
    parser.add_argument("--all", action="store_true", help="scan all tracked text")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    repo_root = Path.cwd().resolve()
    if args.all:
        issues = audit_repository(repo_root)
    else:
        head = resolve_head(args.head, repo_root)
        base = resolve_base(args.base, head, repo_root)
        issues = audit_delta(base, head, args.diff_mode, repo_root)

    if issues:
        print("PUBLIC_AUDIT=FAIL")
        for issue in issues:
            print(issue.receipt())
        return 1
    print("PUBLIC_AUDIT=PASS")
    print("PUBLIC_AUDIT_SCOPE=DETERMINISTIC_BASELINE_NOT_SEMANTIC_PROOF")
    return 0


if __name__ == "__main__":
    sys.exit(main())
