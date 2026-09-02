#!/usr/bin/env python3
"""Bounded local candidate and protected GitHub workflow helpers for RunnerMesh."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Mapping, Protocol, Sequence


REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from tools.dev.portability import run_portability  # noqa: E402
from tools.quality.change_classifier import (  # noqa: E402
    ChangeClass,
    classify_paths,
    git_changed_paths,
    resolve_base,
    resolve_head,
)


EXPECTED_REPOSITORY = "jerry-devlab/runnermesh"
CI_WORKFLOW = "ci.yml"
CI_GATE_NAME = "CI Gate"
FULL_SHA = re.compile(r"^[0-9a-fA-F]{40}$")
TERMINAL_FAILURES = {
    "ACTION_REQUIRED",
    "CANCELLED",
    "FAILURE",
    "SKIPPED",
    "STALE",
    "STARTUP_FAILURE",
    "TIMED_OUT",
}


class ToolError(RuntimeError):
    """A concise, credential-safe tooling failure."""


class DeadlineExceeded(ToolError):
    """A bounded external command exhausted its caller-provided deadline."""


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str = ""
    stderr: str = ""


class Runner(Protocol):
    def which(self, command: str) -> str | None: ...

    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
        timeout: float | None = None,
    ) -> CommandResult: ...


class SubprocessRunner:
    def which(self, command: str) -> str | None:
        return shutil.which(command)

    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
        timeout: float | None = None,
    ) -> CommandResult:
        try:
            completed = subprocess.run(
                list(args),
                cwd=cwd,
                env=dict(env) if env is not None else None,
                check=False,
                capture_output=True,
                text=True,
                encoding="utf-8",
                timeout=timeout,
            )
        except subprocess.TimeoutExpired as error:
            raise DeadlineExceeded(f"{args[0]} exceeded its bounded deadline") from error
        except OSError as error:
            raise ToolError(f"unable to start {args[0]}") from error
        return CommandResult(
            completed.returncode, completed.stdout.strip(), completed.stderr.strip()
        )


def _require_success(result: CommandResult, operation: str) -> str:
    if result.returncode != 0:
        raise ToolError(f"{operation} failed with exit code {result.returncode}")
    return result.stdout


def _validate_sha(value: str, field: str) -> str:
    if not FULL_SHA.fullmatch(value):
        raise ToolError(f"{field} must be a full 40-character commit SHA")
    return value.lower()


def timing_filename(identity: str) -> str:
    sha = _validate_sha(identity, "timing identity")
    return f"runnermesh-train-timing-{sha}.json"


def _iso(epoch: float) -> str:
    return datetime.fromtimestamp(epoch, timezone.utc).isoformat().replace("+00:00", "Z")


def _parse_iso(value: str | None) -> float | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
    except ValueError:
        return None


def _emit(fields: Mapping[str, object]) -> None:
    for key, value in fields.items():
        if isinstance(value, bool):
            rendered = str(value).lower()
        else:
            rendered = str(value)
        print(f"{key}={rendered}")


def normalize_repository_identity(remote_url: str) -> str | None:
    value = remote_url.strip().rstrip("/")
    patterns = (
        r"https?://github\.com/([^/]+/[^/]+?)(?:\.git)?$",
        r"ssh://git@github\.com/([^/]+/[^/]+?)(?:\.git)?$",
        r"git@github\.com:([^/]+/[^/]+?)(?:\.git)?$",
    )
    for pattern in patterns:
        match = re.fullmatch(pattern, value, flags=re.IGNORECASE)
        if match:
            return match.group(1).lower()
    return None


class Repository:
    def __init__(self, root: Path, runner: Runner | None = None):
        self.root = root.resolve()
        self.runner = runner or SubprocessRunner()

    def git(self, args: Sequence[str], *, timeout: float | None = None) -> str:
        result = self.runner.run(("git", *args), cwd=self.root, timeout=timeout)
        return _require_success(result, "git command")

    def assert_identity(self) -> None:
        remote = self.git(("remote", "get-url", "origin"))
        actual = normalize_repository_identity(remote)
        if actual != EXPECTED_REPOSITORY:
            raise ToolError(
                f"wrong repository: expected {EXPECTED_REPOSITORY}, got {actual or 'unrecognized'}"
            )

    def head(self) -> str:
        return self.git(("rev-parse", "HEAD"))

    def branch(self) -> str:
        return self.git(("branch", "--show-current")) or "DETACHED"

    def clean(self) -> bool:
        return not bool(self.git(("status", "--porcelain")))

    def authoritative_main(self) -> str:
        output = self.git(("ls-remote", "--exit-code", "origin", "refs/heads/main"))
        lines = [line for line in output.splitlines() if line.strip()]
        if len(lines) != 1:
            raise ToolError("origin main did not resolve uniquely")
        return _validate_sha(lines[0].split()[0], "origin main")

    def fetch_main(self, *, timeout: float | None = None) -> None:
        self.git(("fetch", "origin", "main"), timeout=timeout)

    def timing_path(self, identity: str) -> Path:
        value = self.git(
            (
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                timing_filename(identity),
            )
        )
        return Path(value)


class GithubClient:
    def __init__(self, repo_root: Path, runner: Runner | None = None):
        self.repo_root = repo_root
        self.runner = runner or SubprocessRunner()

    def available(self, *, timeout: float | None = None) -> bool:
        if self.runner.which("gh") is None:
            return False
        result = self.runner.run(
            ("gh", "auth", "status", "--hostname", "github.com"),
            cwd=self.repo_root,
            timeout=timeout,
        )
        return result.returncode == 0

    def _json(
        self, args: Sequence[str], operation: str, *, timeout: float | None = None
    ) -> Any:
        result = self.runner.run(("gh", *args), cwd=self.repo_root, timeout=timeout)
        output = _require_success(result, operation)
        try:
            return json.loads(output)
        except json.JSONDecodeError as error:
            raise ToolError(f"{operation} returned invalid JSON") from error

    def pr_view(self, number: int, *, timeout: float | None = None) -> dict[str, Any]:
        value = self._json(
            (
                "pr",
                "view",
                str(number),
                "--repo",
                EXPECTED_REPOSITORY,
                "--json",
                "number,state,headRefOid,baseRefName,baseRefOid,mergeable,mergeStateStatus,statusCheckRollup,mergeCommit,url",
            ),
            "PR lookup",
            timeout=timeout,
        )
        if not isinstance(value, dict):
            raise ToolError("PR lookup returned an unexpected shape")
        return value

    def open_prs(self, branch: str) -> list[dict[str, Any]]:
        value = self._json(
            (
                "pr",
                "list",
                "--repo",
                EXPECTED_REPOSITORY,
                "--head",
                branch,
                "--state",
                "open",
                "--limit",
                "2",
                "--json",
                "number,state,headRefOid,baseRefName,url",
            ),
            "open PR lookup",
        )
        return value if isinstance(value, list) else []

    def main_sha(self, *, timeout: float | None = None) -> str:
        value = self._json(
            ("api", f"repos/{EXPECTED_REPOSITORY}/git/ref/heads/main"),
            "main reference lookup",
            timeout=timeout,
        )
        try:
            return _validate_sha(value["object"]["sha"], "GitHub main")
        except (KeyError, TypeError) as error:
            raise ToolError("main reference lookup returned an unexpected shape") from error

    def workflow_runs(
        self, sha: str, *, timeout: float | None = None
    ) -> list[dict[str, Any]]:
        value = self._json(
            (
                "run",
                "list",
                "--repo",
                EXPECTED_REPOSITORY,
                "--workflow",
                CI_WORKFLOW,
                "--branch",
                "main",
                "--event",
                "push",
                "--commit",
                sha,
                "--limit",
                "10",
                "--json",
                "databaseId,event,headSha,status,conclusion,startedAt,updatedAt,url",
            ),
            "workflow run lookup",
            timeout=timeout,
        )
        return value if isinstance(value, list) else []

    def run_view(
        self, run_id: int, *, timeout: float | None = None
    ) -> dict[str, Any]:
        value = self._json(
            (
                "run",
                "view",
                str(run_id),
                "--repo",
                EXPECTED_REPOSITORY,
                "--json",
                "databaseId,event,headSha,status,conclusion,startedAt,updatedAt,jobs,url",
            ),
            "workflow run detail lookup",
            timeout=timeout,
        )
        if not isinstance(value, dict):
            raise ToolError("workflow run lookup returned an unexpected shape")
        return value

    def protection(
        self,
        *,
        deadline: float | None = None,
        clock: Callable[[], float] = time.time,
    ) -> dict[str, Any] | None:
        summaries = self._json(
            ("api", f"repos/{EXPECTED_REPOSITORY}/rulesets"),
            "ruleset lookup",
            timeout=_remaining_deadline(deadline, clock),
        )
        _remaining_deadline(deadline, clock)
        if not isinstance(summaries, list):
            return None
        selected = next(
            (item for item in summaries if item.get("name") == "protect-main"), None
        )
        if not selected or not isinstance(selected.get("id"), int):
            return None
        detail = self._json(
            ("api", f"repos/{EXPECTED_REPOSITORY}/rulesets/{selected['id']}"),
            "ruleset detail lookup",
            timeout=_remaining_deadline(deadline, clock),
        )
        _remaining_deadline(deadline, clock)
        return detail if isinstance(detail, dict) else None

    def commit_parents(self, sha: str) -> list[str]:
        value = self._json(
            ("api", f"repos/{EXPECTED_REPOSITORY}/git/commits/{sha}"),
            "merge commit lookup",
        )
        parents = value.get("parents") if isinstance(value, Mapping) else None
        if not isinstance(parents, list):
            raise ToolError("merge commit lookup returned an unexpected shape")
        try:
            return [
                _validate_sha(str(parent["sha"]), "merge parent")
                for parent in parents
            ]
        except (KeyError, TypeError) as error:
            raise ToolError("merge commit lookup returned an unexpected shape") from error

    def merge(self, number: int, expected_head: str) -> CommandResult:
        return self.runner.run(
            build_merge_command(number, expected_head), cwd=self.repo_root
        )


def build_merge_command(number: int, expected_head: str) -> tuple[str, ...]:
    return (
        "gh",
        "pr",
        "merge",
        str(number),
        "--repo",
        EXPECTED_REPOSITORY,
        "--merge",
        "--match-head-commit",
        expected_head,
    )


def reconcile_merge_result(
    merge_result: CommandResult,
    pr: Mapping[str, Any],
    expected_head: str,
    authoritative_main: str,
) -> tuple[str | None, list[str]]:
    """Bind a possibly ambiguous merge attempt to the PR's actual merge commit."""

    errors: list[str] = []
    if str(pr.get("state") or "").upper() != "MERGED":
        errors.append("PR_NOT_MERGED")
        if merge_result.returncode != 0:
            errors.append("MERGE_COMMAND_FAILED")
    if str(pr.get("headRefOid") or "").lower() != expected_head:
        errors.append("STALE_HEAD")
    merge_commit_value = pr.get("mergeCommit")
    merge_commit_raw = (
        merge_commit_value.get("oid")
        if isinstance(merge_commit_value, Mapping)
        else None
    )
    try:
        merge_commit = _validate_sha(str(merge_commit_raw or ""), "PR merge commit")
    except ToolError:
        merge_commit = None
        errors.append("MERGE_COMMIT_UNAVAILABLE")
    if merge_commit is not None and authoritative_main != merge_commit:
        errors.append("AUTHORITATIVE_MAIN_MISMATCH")
    return merge_commit, errors


def _check_name(check: Mapping[str, Any]) -> str:
    return str(check.get("name") or check.get("context") or "UNKNOWN_CHECK")


def check_state(check: Mapping[str, Any] | None) -> str:
    if check is None:
        return "PENDING"
    conclusion = str(check.get("conclusion") or "").upper()
    state = str(check.get("state") or "").upper()
    status = str(check.get("status") or "").upper()
    if conclusion == "SUCCESS" or state == "SUCCESS":
        return "PASS"
    if conclusion in TERMINAL_FAILURES or state in TERMINAL_FAILURES:
        return "FAIL"
    if status == "COMPLETED" and conclusion:
        return "FAIL"
    return "PENDING"


def find_ci_gate(pr: Mapping[str, Any]) -> Mapping[str, Any] | None:
    checks = pr.get("statusCheckRollup") or []
    return next((item for item in checks if _check_name(item) == CI_GATE_NAME), None)


def failed_check_names(pr: Mapping[str, Any]) -> list[str]:
    checks = pr.get("statusCheckRollup") or []
    return sorted(
        {
            _check_name(check)
            for check in checks
            if check_state(check) == "FAIL"
        }
    )


@dataclass(frozen=True)
class PrWaitResult:
    status: str
    started_at: float
    ended_at: float
    failed_checks: tuple[str, ...] = ()


def wait_for_pr(
    client: GithubClient,
    number: int,
    expected_head: str,
    *,
    timeout: float,
    interval: float,
    clock: Callable[[], float] = time.time,
    sleeper: Callable[[float], None] = time.sleep,
    progress: Callable[[str], None] = print,
    started_at: float | None = None,
) -> PrWaitResult:
    started = clock() if started_at is None else started_at
    deadline = started + timeout
    while True:
        remaining = deadline - clock()
        if remaining <= 0:
            return PrWaitResult("TIMEOUT", started, clock())
        try:
            pr = client.pr_view(number, timeout=remaining)
        except DeadlineExceeded:
            return PrWaitResult("TIMEOUT", started, clock())
        observed_at = clock()
        if observed_at >= deadline:
            return PrWaitResult("TIMEOUT", started, observed_at)
        actual_head = str(pr.get("headRefOid") or "").lower()
        if actual_head != expected_head:
            return PrWaitResult("STALE_HEAD", started, clock())
        if str(pr.get("state") or "").upper() != "OPEN":
            return PrWaitResult("PR_NOT_OPEN", started, clock())

        gate = find_ci_gate(pr)
        state = check_state(gate)
        if state == "PASS":
            completed_at = _parse_iso(str(gate.get("completedAt") or "")) if gate else None
            return PrWaitResult("PASS", started, completed_at or clock())
        if state == "FAIL":
            return PrWaitResult(
                "FAIL", started, clock(), tuple(failed_check_names(pr))
            )

        now = clock()
        if now >= deadline:
            return PrWaitResult("TIMEOUT", started, now)
        progress(f"PR_CI=PENDING ELAPSED_SECONDS={int(now - started)}")
        sleeper(min(interval, max(0.0, deadline - now)))


def _ref_pattern_matches_main(pattern: object) -> bool:
    value = str(pattern)
    # Fail closed instead of trying to reproduce GitHub's pathname-aware
    # ruleset glob dialect. These are the only forms that unambiguously prove
    # the fixed RunnerMesh target is included (or excluded).
    return value in {"~ALL", "refs/heads/main"}


def protection_is_effective(detail: Mapping[str, Any] | None) -> bool:
    if not detail or detail.get("enforcement") != "active":
        return False
    if detail.get("bypass_actors"):
        return False
    conditions = detail.get("conditions")
    if not isinstance(conditions, Mapping):
        return False
    ref_conditions = conditions.get("ref_name")
    if not isinstance(ref_conditions, Mapping):
        return False
    includes = ref_conditions.get("include", [])
    excludes = ref_conditions.get("exclude", [])
    if not isinstance(includes, list) or not isinstance(excludes, list):
        return False
    # The token follows the repository's mutable default-branch setting. Without
    # resolving that setting in the same protection snapshot, it cannot prove
    # that the ruleset protects the fixed RunnerMesh target branch.
    if "~DEFAULT_BRANCH" in includes or "~DEFAULT_BRANCH" in excludes:
        return False
    if not any(_ref_pattern_matches_main(pattern) for pattern in includes):
        return False
    # Any exclusion requires GitHub-compatible glob evaluation to prove that it
    # cannot remove main from the ruleset. This bounded helper deliberately
    # supports only unexcluded protection and otherwise fails closed.
    if excludes:
        return False
    rules = detail.get("rules") or []
    types = {rule.get("type") for rule in rules}
    if "merge_queue" in types:
        return False
    if not {
        "deletion",
        "pull_request",
        "non_fast_forward",
        "required_status_checks",
    }.issubset(types):
        return False
    status_rule = next(
        (rule for rule in rules if rule.get("type") == "required_status_checks"), {}
    )
    contexts = {
        item.get("context")
        for item in status_rule.get("parameters", {}).get("required_status_checks", [])
    }
    return CI_GATE_NAME in contexts


def protection_supports_safe_merge(detail: Mapping[str, Any] | None) -> bool:
    """Accept active no-bypass protection; exact-base freshness is checked separately."""

    return protection_is_effective(detail)


def next_merge_allowed(
    main_ci: str, protection: Mapping[str, Any] | None
) -> bool:
    """Combine prior-main health and repository protection preconditions."""

    return main_ci == "PASS" and protection_supports_safe_merge(protection)


def validate_merge_preconditions(
    pr: Mapping[str, Any],
    expected_head: str,
    *,
    expected_base: str,
    current_main: str,
    repository_ok: bool,
    protection_ok: bool,
    current_main_health: str,
    checked_out_head: str | None = None,
    worktree_clean: bool = True,
) -> list[str]:
    errors: list[str] = []
    if not repository_ok:
        errors.append("WRONG_REPOSITORY")
    if str(pr.get("headRefOid") or "").lower() != expected_head:
        errors.append("STALE_HEAD")
    if checked_out_head is not None and checked_out_head.lower() != expected_head:
        errors.append("CHECKED_OUT_HEAD_MISMATCH")
    if not worktree_clean:
        errors.append("WORKTREE_NOT_CLEAN")
    if str(pr.get("state") or "").upper() != "OPEN":
        errors.append("PR_NOT_OPEN")
    if pr.get("baseRefName") != "main":
        errors.append("WRONG_BASE")
    pr_base = str(pr.get("baseRefOid") or "").lower()
    if pr_base != expected_base or current_main != expected_base:
        errors.append("BASE_STALE")
    if (
        str(pr.get("mergeable") or "").upper() != "MERGEABLE"
        or str(pr.get("mergeStateStatus") or "").upper() != "CLEAN"
    ):
        errors.append("PR_NOT_MERGEABLE")
    if check_state(find_ci_gate(pr)) != "PASS":
        errors.append("CI_GATE_NOT_PASS")
    if not protection_ok:
        errors.append("MAIN_PROTECTION_NOT_EFFECTIVE")
    if current_main_health != "PASS":
        errors.append("CURRENT_MAIN_NOT_HEALTHY")
    return errors


def validate_merge_parents(
    parents: Sequence[str], current_main: str, expected_head: str
) -> list[str]:
    errors: list[str] = []
    if len(parents) != 2:
        return ["MERGE_PARENT_SHAPE"]
    if parents[0] != current_main:
        errors.append("UNHEALTHY_BASE_PARENT")
    if parents[1] != expected_head:
        errors.append("WRONG_HEAD_PARENT")
    return errors


def _job_conclusion(jobs: Sequence[Mapping[str, Any]], name: str) -> str | None:
    job = next((item for item in jobs if item.get("name") == name), None)
    return str(job.get("conclusion") or "").lower() if job else None


def validate_main_jobs(
    run: Mapping[str, Any], change_class: ChangeClass
) -> tuple[bool, list[str]]:
    jobs = run.get("jobs") or []
    required = ("Change classification", "Fast Gate", CI_GATE_NAME)
    problems = [
        name for name in required if _job_conclusion(jobs, name) != "success"
    ]
    format_result = _job_conclusion(jobs, "Format")
    cargo_jobs = [
        job for job in jobs if str(job.get("name") or "").startswith("Cargo tests + Clippy")
    ]
    if change_class is ChangeClass.DOCS_ONLY:
        lightweight = format_result == "skipped" and bool(cargo_jobs) and all(
            job.get("conclusion") == "skipped" for job in cargo_jobs
        )
        full = format_result == "success" and all(
            _job_conclusion(jobs, name) == "success"
            for name in (
                "Cargo tests + Clippy (windows-latest)",
                "Cargo tests + Clippy (ubuntu-latest)",
            )
        )
        if not lightweight and not full:
            problems.append("Docs CI expected lightweight skips or full cross-platform success")
    else:
        if format_result != "success":
            problems.append("Format expected success")
        for name in (
            "Cargo tests + Clippy (windows-latest)",
            "Cargo tests + Clippy (ubuntu-latest)",
        ):
            if _job_conclusion(jobs, name) != "success":
                problems.append(name)
    return not problems, problems


def _remaining_deadline(deadline: float | None, clock: Callable[[], float]) -> float | None:
    if deadline is None:
        return None
    remaining = deadline - clock()
    if remaining <= 0:
        raise DeadlineExceeded("GitHub lookup exceeded its bounded deadline")
    return remaining


def _latest_exact_run(
    client: GithubClient,
    expected_main: str,
    *,
    deadline: float | None = None,
    clock: Callable[[], float] = time.time,
) -> dict[str, Any] | None:
    runs = [
        run
        for run in client.workflow_runs(
            expected_main, timeout=_remaining_deadline(deadline, clock)
        )
        if str(run.get("headSha") or "").lower() == expected_main
        and run.get("event") == "push"
    ]
    _remaining_deadline(deadline, clock)
    if not runs:
        return None
    selected = runs[0]
    result = client.run_view(
        int(selected["databaseId"]), timeout=_remaining_deadline(deadline, clock)
    )
    _remaining_deadline(deadline, clock)
    return result


@dataclass(frozen=True)
class MainWaitResult:
    status: str
    started_at: float
    ended_at: float
    run_started_at: float | None = None
    problems: tuple[str, ...] = ()


def wait_for_main(
    client: GithubClient,
    expected_main: str,
    change_class: ChangeClass,
    *,
    timeout: float,
    interval: float,
    clock: Callable[[], float] = time.time,
    sleeper: Callable[[float], None] = time.sleep,
    progress: Callable[[str], None] = print,
    started_at: float | None = None,
) -> MainWaitResult:
    started = clock() if started_at is None else started_at
    deadline = started + timeout
    while True:
        try:
            current_main = client.main_sha(
                timeout=_remaining_deadline(deadline, clock)
            )
            _remaining_deadline(deadline, clock)
            run = _latest_exact_run(
                client, expected_main, deadline=deadline, clock=clock
            )
        except DeadlineExceeded:
            return MainWaitResult("TIMEOUT", started, clock())
        if current_main != expected_main:
            return MainWaitResult("STALE_MAIN", started, clock())
        if run is not None and str(run.get("status") or "").lower() == "completed":
            run_start = _parse_iso(str(run.get("startedAt") or ""))
            run_end = _parse_iso(str(run.get("updatedAt") or "")) or clock()
            if str(run.get("conclusion") or "").lower() != "success":
                problems = tuple(
                    sorted(
                        str(job.get("name"))
                        for job in run.get("jobs") or []
                        if job.get("conclusion") not in {"success", "skipped"}
                    )
                )
                terminal_status = "FAIL"
            else:
                healthy, validation_problems = validate_main_jobs(run, change_class)
                terminal_status = "PASS" if healthy else "FAIL"
                problems = tuple(validation_problems)
            try:
                final_main = client.main_sha(
                    timeout=_remaining_deadline(deadline, clock)
                )
                _remaining_deadline(deadline, clock)
            except DeadlineExceeded:
                return MainWaitResult("TIMEOUT", started, clock())
            if final_main != expected_main:
                return MainWaitResult("STALE_MAIN", started, clock())
            return MainWaitResult(
                terminal_status,
                started,
                run_end,
                run_start,
                problems,
            )

        now = clock()
        if now >= deadline:
            return MainWaitResult("TIMEOUT", started, now)
        progress(f"MAIN_CI=PENDING ELAPSED_SECONDS={int(now - started)}")
        sleeper(min(interval, max(0.0, deadline - now)))


def calculate_timing_fields(values: Mapping[str, float]) -> dict[str, int]:
    pairs = {
        "LOCAL_VALIDATION_SECONDS": ("candidate_start", "candidate_ready"),
        "PR_CI_SECONDS": ("pr_wait_start", "pr_ci_end"),
        "MERGE_WAIT_SECONDS": ("pr_ci_end", "merge_time"),
        "MAIN_CI_SECONDS": ("main_ci_start", "main_ci_end"),
        "TOTAL_PIPELINE_SECONDS": ("candidate_start", "main_ci_end"),
    }
    result: dict[str, int] = {}
    for field, (start, end) in pairs.items():
        if start in values and end in values:
            result[field] = max(0, round(values[end] - values[start]))
    return result


def timing_values_for_head(
    values: Mapping[str, Any], expected_head: str
) -> dict[str, Any]:
    """Discard unrelated pipeline telemetry instead of emitting stale timings."""

    candidate_head = values.get("candidate_head")
    if candidate_head is not None and candidate_head != expected_head:
        return {}
    return dict(values)


class TimingStore:
    def __init__(self, path: Path):
        self.path = path

    def load(self) -> dict[str, Any]:
        try:
            value = json.loads(self.path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {}
        return value if isinstance(value, dict) else {}

    def replace(self, values: Mapping[str, Any]) -> dict[str, Any]:
        data = dict(values)
        self._write(data)
        return data

    def update(self, **values: Any) -> dict[str, Any]:
        data = self.load()
        data.update(values)
        self._write(data)
        return data

    def _write(self, data: Mapping[str, Any]) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        temporary = self.path.with_suffix(".tmp")
        temporary.write_text(
            json.dumps(data, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
        temporary.replace(self.path)


def _main_run_health(
    client: GithubClient, sha: str, change_class: ChangeClass
) -> str:
    run = _latest_exact_run(client, sha)
    if run is None:
        return "UNKNOWN"
    if str(run.get("status") or "").lower() != "completed":
        return "PENDING"
    if str(run.get("conclusion") or "").lower() != "success":
        return "FAIL"
    healthy, _ = validate_main_jobs(run, change_class)
    return "PASS" if healthy else "FAIL"


def classify_main_commit(repository: Repository, sha: str) -> ChangeClass:
    repository.fetch_main()
    head = resolve_head(sha, REPO_ROOT)
    base = resolve_base(None, head, REPO_ROOT)
    paths = git_changed_paths(base, head, "direct", REPO_ROOT)
    return classify_paths(paths)


def command_candidate(args: argparse.Namespace) -> int:
    repository = Repository(REPO_ROOT)
    repository.assert_identity()
    if not repository.clean():
        raise ToolError("candidate requires a clean worktree bound to a commit")
    head = resolve_head("HEAD", REPO_ROOT)
    base = resolve_base(args.base, head, REPO_ROOT)
    paths = git_changed_paths(base, head, "merge-base", REPO_ROOT)
    change_class = classify_paths(paths)
    started = time.time()
    timing = TimingStore(repository.timing_path(head))
    data = timing.replace(
        {
            "candidate_start": started,
            "candidate_head": head,
            "base_sha": base,
        }
    )
    _emit(
        {
            "CANDIDATE_START": _iso(started),
            "BASE_SHA": base,
            "HEAD_SHA": head,
            "CHANGE_CLASS": change_class.value,
        }
    )
    command = (
        sys.executable,
        "tools/quality/fast_gate.py",
        "--base",
        base,
        "--head",
        head,
        "--full",
    )
    completed = subprocess.run(list(command), cwd=REPO_ROOT, check=False)
    if completed.returncode != 0:
        ended = time.time()
        data = timing.update(candidate_ready=ended)
        _emit(
            {
                "CANDIDATE_READY": _iso(ended),
                "LOCAL_GATE": "FAIL",
                **calculate_timing_fields(data),
            }
        )
        return completed.returncode

    portability = run_portability(change_class, args.portability, REPO_ROOT)
    ended = time.time()
    if repository.head() != head or not repository.clean():
        data = timing.update(candidate_ready=ended, portability=portability.status)
        _emit(
            {
                "CANDIDATE_READY": _iso(ended),
                "LOCAL_GATE": "FAIL",
                "CANDIDATE_STATE": "STALE",
                "PORTABILITY": portability.status,
                **calculate_timing_fields(data),
            }
        )
        return 1
    data = timing.update(candidate_ready=ended, portability=portability.status)
    _emit(
        {
            "CANDIDATE_READY": _iso(ended),
            "LOCAL_GATE": "PASS",
            "PORTABILITY": portability.status,
            "PORTABILITY_DETAIL": portability.detail,
            **calculate_timing_fields(data),
        }
    )
    return 1 if portability.status == "FAIL" else 0


def command_health(args: argparse.Namespace) -> int:
    repository = Repository(REPO_ROOT)
    repository.assert_identity()
    head = repository.head()
    branch = repository.branch()
    origin_main = repository.authoritative_main()
    roadmap = (REPO_ROOT / "goals/RM-V0_1-ROADMAP.md").read_text(encoding="utf-8")
    match = re.search(r"Roadmap v(\d+)", roadmap)
    ledger_path = REPO_ROOT / "goals/RM-V0_1-EXECUTION-STATUS.md"
    ledger_state = "AVAILABLE" if ledger_path.exists() else "UNAVAILABLE"

    fields: dict[str, object] = {
        "HEAD": head,
        "ORIGIN_MAIN": origin_main,
        "WORKTREE_CLEAN": repository.clean(),
        "BRANCH": branch,
        "ROADMAP_VERSION": f"v{match.group(1)}" if match else "UNKNOWN",
        "LEDGER_STATE": ledger_state,
    }
    client = GithubClient(REPO_ROOT)
    gh_available = client.available()
    fields["GH_AVAILABLE"] = gh_available
    if not gh_available:
        fields.update(
            {
                "OPEN_PR": "N/A",
                "PR_HEAD": "N/A",
                "PR_STATE": "UNKNOWN",
                "CI_GATE": "UNKNOWN",
                "MAIN_HEALTH": "UNKNOWN",
                "NEXT_MERGE_ALLOWED": False,
            }
        )
        _emit(fields)
        return 0

    prs = client.open_prs(branch)
    pr = prs[0] if len(prs) == 1 else None
    merge_ready = False
    if pr is not None:
        detailed_pr = client.pr_view(int(pr["number"]))
        merge_ready = (
            repository.clean()
            and str(detailed_pr.get("state") or "").upper() == "OPEN"
            and str(detailed_pr.get("headRefOid") or "").lower() == head
            and detailed_pr.get("baseRefName") == "main"
            and str(detailed_pr.get("baseRefOid") or "").lower() == origin_main
            and str(detailed_pr.get("mergeable") or "").upper() == "MERGEABLE"
            and str(detailed_pr.get("mergeStateStatus") or "").upper() == "CLEAN"
            and check_state(find_ci_gate(detailed_pr)) == "PASS"
        )
        fields.update(
            {
                "OPEN_PR": pr["number"],
                "PR_HEAD": pr.get("headRefOid") or "N/A",
                "PR_STATE": pr.get("state") or "UNKNOWN",
                "CI_GATE": check_state(find_ci_gate(detailed_pr)),
                "BASE_STALE": str(detailed_pr.get("baseRefOid") or "").lower()
                != origin_main,
            }
        )
    else:
        fields.update(
            {
                "OPEN_PR": "N/A" if not prs else "AMBIGUOUS",
                "PR_HEAD": "N/A",
                "PR_STATE": "NONE" if not prs else "AMBIGUOUS",
                "CI_GATE": "N/A" if not prs else "UNKNOWN",
            }
        )
    main_class = classify_main_commit(repository, origin_main)
    main_health = _main_run_health(client, origin_main, main_class)
    protection = client.protection()
    protection_ok = protection_is_effective(protection)
    safe_merge_protection = protection_supports_safe_merge(protection)
    fields["MAIN_HEALTH"] = main_health
    fields["MAIN_PROTECTION"] = "PASS" if protection_ok else "FAIL"
    fields["SAFE_MERGE_PROTECTION"] = (
        "PASS" if safe_merge_protection else "FAIL"
    )
    fields["NEXT_MERGE_ALLOWED"] = (
        merge_ready and next_merge_allowed(main_health, protection)
    )
    _emit(fields)
    return 0


def command_wait_pr(args: argparse.Namespace) -> int:
    expected_head = _validate_sha(args.expected_head, "expected head")
    started = time.time()
    deadline = started + args.timeout
    repository = Repository(REPO_ROOT)
    repository.assert_identity()
    if repository.head().lower() != expected_head:
        _emit({"PR_CI": "STALE_HEAD", "STALE_HEAD": True})
        return 1
    client = GithubClient(REPO_ROOT)
    try:
        available = client.available(
            timeout=_remaining_deadline(deadline, time.time)
        )
        _remaining_deadline(deadline, time.time)
    except DeadlineExceeded:
        _emit({"GH_AVAILABLE": "UNKNOWN", "PR_CI": "TIMEOUT"})
        return 1
    if not available:
        _emit({"GH_AVAILABLE": False, "PR_CI": "BLOCKED"})
        return 2
    timing = TimingStore(repository.timing_path(expected_head))
    bound = timing_values_for_head(timing.load(), expected_head)
    bound.update(pr_wait_start=started, pr=args.pr, expected_head=expected_head)
    timing.replace(bound)
    _emit({"PR_WAIT_START": _iso(started), "EXPECTED_HEAD": expected_head})
    result = wait_for_pr(
        client,
        args.pr,
        expected_head,
        timeout=args.timeout,
        interval=args.interval,
        started_at=started,
    )
    if result.status == "PASS" and repository.head().lower() != expected_head:
        result = PrWaitResult("STALE_HEAD", result.started_at, time.time())
    data = timing.update(pr_ci_end=result.ended_at)
    fields: dict[str, object] = {
        "PR_CI_END": _iso(result.ended_at),
        "PR_CI": result.status,
        "STALE_HEAD": result.status == "STALE_HEAD",
        **calculate_timing_fields(data),
    }
    if result.failed_checks:
        fields["FAILED_CHECKS"] = ",".join(result.failed_checks)
    _emit(fields)
    return 0 if result.status == "PASS" else 1


def command_merge(args: argparse.Namespace) -> int:
    expected_head = _validate_sha(args.expected_head, "expected head")
    expected_base = _validate_sha(args.expected_base, "expected base")
    repository = Repository(REPO_ROOT)
    repository.assert_identity()
    client = GithubClient(REPO_ROOT)
    if not client.available():
        _emit({"GH_AVAILABLE": False, "MERGE": "BLOCKED"})
        return 2
    pr = client.pr_view(args.pr)
    protection_ok = protection_supports_safe_merge(client.protection())
    current_main = client.main_sha()
    current_class = classify_main_commit(repository, current_main)
    current_health = _main_run_health(client, current_main, current_class)
    errors = validate_merge_preconditions(
        pr,
        expected_head,
        expected_base=expected_base,
        current_main=current_main,
        repository_ok=True,
        protection_ok=protection_ok,
        current_main_health=current_health,
        checked_out_head=repository.head(),
        worktree_clean=repository.clean(),
    )
    if errors:
        _emit(
            {
                "MERGE": "REFUSED",
                "BASE_STALE": "BASE_STALE" in errors,
                "REFUSAL_REASONS": ",".join(errors),
            }
        )
        return 1
    if repository.head().lower() != expected_head or not repository.clean():
        _emit(
            {
                "MERGE": "REFUSED",
                "REFUSAL_REASONS": "CHECKED_OUT_CANDIDATE_CHANGED",
            }
        )
        return 1
    # Refresh every mutable remote precondition immediately before dispatch.
    # The final remote read is authoritative main, minimizing the non-strict
    # ruleset's base-staleness window without mutating or rebasing the PR.
    pr = client.pr_view(args.pr)
    protection_ok = protection_supports_safe_merge(client.protection())
    current_main = client.main_sha()
    errors = validate_merge_preconditions(
        pr,
        expected_head,
        expected_base=expected_base,
        current_main=current_main,
        repository_ok=True,
        protection_ok=protection_ok,
        current_main_health=current_health,
        checked_out_head=repository.head(),
        worktree_clean=repository.clean(),
    )
    if errors:
        _emit(
            {
                "MERGE": "REFUSED",
                "BASE_STALE": "BASE_STALE" in errors,
                "REFUSAL_REASONS": ",".join(errors),
            }
        )
        return 1
    merge_result = client.merge(args.pr, expected_head)
    merged_pr = client.pr_view(args.pr)
    authoritative_main = client.main_sha()
    final_main, reconciliation_errors = reconcile_merge_result(
        merge_result, merged_pr, expected_head, authoritative_main
    )
    if reconciliation_errors or final_main is None:
        _emit(
            {
                "MERGE": "RECONCILIATION_REQUIRED",
                "REFUSAL_REASONS": ",".join(reconciliation_errors),
            }
        )
        return 1
    parent_errors = validate_merge_parents(
        client.commit_parents(final_main), current_main, expected_head
    )
    if client.main_sha() != final_main:
        parent_errors.append("AUTHORITATIVE_MAIN_CHANGED")
    if parent_errors:
        _emit(
            {
                "MERGE": "RECONCILIATION_REQUIRED",
                "REFUSAL_REASONS": ",".join(parent_errors),
            }
        )
        return 1
    if final_main == current_main:
        raise ToolError("protected merge did not advance authoritative main")
    merged_at = time.time()
    timing = TimingStore(repository.timing_path(expected_head))
    bound = timing_values_for_head(timing.load(), expected_head)
    bound.update(merge_time=merged_at, final_main=final_main)
    data = timing.replace(bound)
    TimingStore(repository.timing_path(final_main)).replace(data)
    _emit(
        {
            "MERGE": "PASS",
            "MERGE_TIME": _iso(merged_at),
            "MAIN_SHA": final_main,
            **calculate_timing_fields(data),
        }
    )
    return 0


def command_wait_main(args: argparse.Namespace) -> int:
    expected_main = _validate_sha(args.expected_main, "expected main")
    started = time.time()
    deadline = started + args.timeout
    repository = Repository(REPO_ROOT)
    repository.assert_identity()
    client = GithubClient(REPO_ROOT)
    try:
        available = client.available(
            timeout=_remaining_deadline(deadline, time.time)
        )
        _remaining_deadline(deadline, time.time)
    except DeadlineExceeded:
        _emit({"GH_AVAILABLE": "UNKNOWN", "MAIN_CI": "TIMEOUT"})
        return 1
    if not available:
        _emit({"GH_AVAILABLE": False, "MAIN_CI": "BLOCKED"})
        return 2
    try:
        current_main = client.main_sha(
            timeout=_remaining_deadline(deadline, time.time)
        )
        _remaining_deadline(deadline, time.time)
    except DeadlineExceeded:
        _emit({"GH_AVAILABLE": True, "MAIN_CI": "TIMEOUT"})
        return 1
    if current_main != expected_main:
        _emit(
            {
                "MAIN_SHA": expected_main,
                "MAIN_CI": "STALE_MAIN",
                "PIPELINE_HEALTH": "FAIL",
                "NEXT_MERGE_ALLOWED": False,
            }
        )
        return 1
    try:
        repository.fetch_main(timeout=_remaining_deadline(deadline, time.time))
        _remaining_deadline(deadline, time.time)
    except DeadlineExceeded:
        _emit({"GH_AVAILABLE": True, "MAIN_CI": "TIMEOUT"})
        return 1
    head = resolve_head(expected_main, REPO_ROOT)
    base = resolve_base(None, head, REPO_ROOT)
    paths = git_changed_paths(base, head, "direct", REPO_ROOT)
    change_class = classify_paths(paths)
    result = wait_for_main(
        client,
        expected_main,
        change_class,
        timeout=args.timeout,
        interval=args.interval,
        started_at=started,
    )
    timing = TimingStore(repository.timing_path(expected_main))
    values: dict[str, Any] = {"main_ci_end": result.ended_at}
    if result.run_started_at is not None:
        values["main_ci_start"] = result.run_started_at
    data = timing.update(**values)
    safe_merge_protection = False
    protection_status = "N/A"
    if result.status == "PASS":
        try:
            protection = client.protection(deadline=deadline)
            safe_merge_protection = protection_supports_safe_merge(protection)
            protection_status = "PASS" if safe_merge_protection else "FAIL"
            final_main = client.main_sha(
                timeout=_remaining_deadline(deadline, time.time)
            )
            _remaining_deadline(deadline, time.time)
        except DeadlineExceeded:
            protection_status = "UNKNOWN"
        else:
            if final_main != expected_main:
                _emit(
                    {
                        "MAIN_SHA": expected_main,
                        "MAIN_CI": "STALE_MAIN",
                        "PIPELINE_HEALTH": "FAIL",
                        "SAFE_MERGE_PROTECTION": protection_status,
                        "NEXT_MERGE_ALLOWED": False,
                        **calculate_timing_fields(data),
                    }
                )
                return 1
    fields: dict[str, object] = {
        "MAIN_SHA": expected_main,
        "CHANGE_CLASS": change_class.value,
        "MAIN_CI": result.status,
        "PIPELINE_HEALTH": "PASS" if result.status == "PASS" else "FAIL",
        "SAFE_MERGE_PROTECTION": protection_status,
        "NEXT_MERGE_ALLOWED": result.status == "PASS" and safe_merge_protection,
        **calculate_timing_fields(data),
    }
    if result.problems:
        fields["FAILED_JOBS"] = ",".join(result.problems)
    _emit(fields)
    return 0 if result.status == "PASS" else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    candidate = subparsers.add_parser("candidate", help="run the settled local candidate gate")
    candidate.add_argument("--base", required=True, help="accepted main commit")
    candidate.add_argument(
        "--portability", choices=("auto", "off"), default="auto"
    )
    candidate.set_defaults(handler=command_candidate)

    health = subparsers.add_parser("health", help="show concise development state")
    health.set_defaults(handler=command_health)

    wait_pr = subparsers.add_parser("wait-pr", help="wait for exact-head PR CI")
    wait_pr.add_argument("--pr", type=int, required=True)
    wait_pr.add_argument("--expected-head", required=True)
    wait_pr.add_argument("--timeout", type=float, default=1800.0)
    wait_pr.add_argument("--interval", type=float, default=15.0)
    wait_pr.set_defaults(handler=command_wait_pr)

    merge = subparsers.add_parser("merge", help="merge through protected GitHub PR flow")
    merge.add_argument("--pr", type=int, required=True)
    merge.add_argument("--expected-head", required=True)
    merge.add_argument("--expected-base", required=True)
    merge.set_defaults(handler=command_merge)

    wait_main = subparsers.add_parser("wait-main", help="wait for exact main CI health")
    wait_main.add_argument("--expected-main", required=True)
    wait_main.add_argument("--timeout", type=float, default=1800.0)
    wait_main.add_argument("--interval", type=float, default=15.0)
    wait_main.set_defaults(handler=command_wait_main)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if getattr(args, "timeout", 1) <= 0:
        raise ToolError("timeout must be positive")
    if getattr(args, "interval", 1) <= 0:
        raise ToolError("interval must be positive")
    return int(args.handler(args))


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ToolError as error:
        print(f"TOOL_ERROR={error}", file=sys.stderr)
        raise SystemExit(2)
