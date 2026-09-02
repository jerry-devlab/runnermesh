import argparse
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import Mapping, Sequence
from unittest import mock

from tools.dev.train import (
    CI_GATE_NAME,
    EXPECTED_REPOSITORY,
    CommandResult,
    GithubClient,
    Repository,
    TimingStore,
    ToolError,
    build_merge_command,
    calculate_timing_fields,
    command_candidate,
    command_merge,
    command_wait_main,
    command_wait_pr,
    _main_run_health,
    normalize_repository_identity,
    next_merge_allowed,
    protection_is_effective,
    protection_supports_safe_merge,
    reconcile_merge_result,
    timing_filename,
    timing_values_for_head,
    validate_main_jobs,
    validate_merge_parents,
    validate_merge_preconditions,
    wait_for_main,
    wait_for_pr,
)
from tools.quality.change_classifier import ChangeClass


HEAD = "a" * 40
MAIN = "b" * 40


def check(name: str, conclusion: str, status: str = "COMPLETED") -> dict:
    return {"name": name, "conclusion": conclusion, "status": status}


def open_pr(head: str = HEAD, gate: str = "SUCCESS") -> dict:
    return {
        "number": 32,
        "state": "OPEN",
        "headRefOid": head,
        "baseRefName": "main",
        "baseRefOid": MAIN,
        "mergeable": "MERGEABLE",
        "mergeStateStatus": "CLEAN",
        "statusCheckRollup": [check(CI_GATE_NAME, gate)],
    }


def protection_detail(*, strict: bool = False) -> dict:
    return {
        "enforcement": "active",
        "bypass_actors": [],
        "conditions": {
            "ref_name": {"include": ["refs/heads/main"], "exclude": []}
        },
        "rules": [
            {"type": "deletion"},
            {"type": "pull_request"},
            {"type": "non_fast_forward"},
            {
                "type": "required_status_checks",
                "parameters": {
                    "strict_required_status_checks_policy": strict,
                    "required_status_checks": [{"context": CI_GATE_NAME}],
                },
            },
        ],
    }


def successful_jobs(change_class: ChangeClass) -> list[dict]:
    jobs = [
        check("Change classification", "success"),
        check("Fast Gate", "success"),
        check(CI_GATE_NAME, "success"),
    ]
    if change_class is ChangeClass.DOCS_ONLY:
        jobs.extend(
            [
                check("Format", "skipped"),
                check("Cargo tests + Clippy (${{ matrix.os }})", "skipped"),
            ]
        )
    else:
        jobs.extend(
            [
                check("Format", "success"),
                check("Cargo tests + Clippy (windows-latest)", "success"),
                check("Cargo tests + Clippy (ubuntu-latest)", "success"),
            ]
        )
    return jobs


class MutableClock:
    def __init__(self) -> None:
        self.value = 0.0

    def __call__(self) -> float:
        return self.value

    def sleep(self, seconds: float) -> None:
        self.value += seconds


class FakeGithub:
    def __init__(
        self,
        prs: list[dict] | None = None,
        run: dict | None = None,
        *,
        clock: MutableClock | None = None,
        call_delay: float = 0.0,
        main_shas: list[str] | None = None,
    ):
        self.prs = list(prs or [])
        self.run = run
        self.clock = clock
        self.call_delay = call_delay
        self.main_shas = list(main_shas or [])

    def _delay(self) -> None:
        if self.clock is not None:
            self.clock.value += self.call_delay

    def pr_view(self, number: int, *, timeout: float | None = None) -> dict:
        self._delay()
        if len(self.prs) > 1:
            return self.prs.pop(0)
        return self.prs[0]

    def main_sha(self, *, timeout: float | None = None) -> str:
        self._delay()
        if len(self.main_shas) > 1:
            return self.main_shas.pop(0)
        if self.main_shas:
            return self.main_shas[0]
        return MAIN

    def workflow_runs(self, sha: str, *, timeout: float | None = None) -> list[dict]:
        self._delay()
        if self.run is None:
            return []
        return [{"databaseId": 7, "headSha": MAIN, "event": "push"}]

    def run_view(self, run_id: int, *, timeout: float | None = None) -> dict:
        self._delay()
        assert self.run is not None
        return self.run


class FakeRunner:
    def __init__(self, remote: str = "https://github.com/example/wrong.git"):
        self.remote = remote

    def which(self, command: str) -> str | None:
        return None

    def run(
        self,
        args: Sequence[str],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
        timeout: float | None = None,
    ) -> CommandResult:
        if tuple(args) == ("git", "remote", "get-url", "origin"):
            return CommandResult(0, self.remote, "")
        return CommandResult(1, "", "unavailable")


class PrWaitTests(unittest.TestCase):
    def test_exact_head_pr_ci_pass(self) -> None:
        result = wait_for_pr(
            FakeGithub([open_pr()]),
            32,
            HEAD,
            timeout=30,
            interval=5,
            progress=lambda _: None,
        )
        self.assertEqual(result.status, "PASS")

    def test_stale_head_refuses(self) -> None:
        result = wait_for_pr(
            FakeGithub([open_pr("c" * 40)]),
            32,
            HEAD,
            timeout=30,
            interval=5,
            progress=lambda _: None,
        )
        self.assertEqual(result.status, "STALE_HEAD")

    def test_pr_ci_failure_has_summary(self) -> None:
        pr = open_pr(gate="FAILURE")
        pr["statusCheckRollup"].append(check("Cargo tests + Clippy (ubuntu-latest)", "FAILURE"))
        result = wait_for_pr(
            FakeGithub([pr]),
            32,
            HEAD,
            timeout=30,
            interval=5,
            progress=lambda _: None,
        )
        self.assertEqual(result.status, "FAIL")
        self.assertIn("Cargo tests + Clippy (ubuntu-latest)", result.failed_checks)

    def test_timeout_is_bounded(self) -> None:
        pending = open_pr(gate="")
        pending["statusCheckRollup"][0]["status"] = "IN_PROGRESS"
        clock = MutableClock()
        result = wait_for_pr(
            FakeGithub([pending]),
            32,
            HEAD,
            timeout=12,
            interval=5,
            clock=clock,
            sleeper=clock.sleep,
            progress=lambda _: None,
        )
        self.assertEqual(result.status, "TIMEOUT")
        self.assertEqual(clock.value, 12)

    def test_completed_call_after_deadline_is_timeout_not_pass(self) -> None:
        clock = MutableClock()
        result = wait_for_pr(
            FakeGithub([open_pr()], clock=clock, call_delay=11),
            32,
            HEAD,
            timeout=10,
            interval=5,
            clock=clock,
            sleeper=clock.sleep,
            progress=lambda _: None,
        )
        self.assertEqual(result.status, "TIMEOUT")


class MergeSafetyTests(unittest.TestCase):
    def test_merge_preconditions_refuse_unmet_requirements(self) -> None:
        pr = open_pr("c" * 40, gate="PENDING")
        pr["baseRefName"] = "release"
        pr["baseRefOid"] = "d" * 40
        pr["mergeable"] = "CONFLICTING"
        pr["mergeStateStatus"] = "DIRTY"
        errors = validate_merge_preconditions(
            pr,
            HEAD,
            expected_base=MAIN,
            current_main=MAIN,
            repository_ok=False,
            protection_ok=False,
            current_main_health="FAIL",
            checked_out_head="e" * 40,
            worktree_clean=False,
        )
        self.assertEqual(
            set(errors),
            {
                "WRONG_REPOSITORY",
                "STALE_HEAD",
                "WRONG_BASE",
                "BASE_STALE",
                "PR_NOT_MERGEABLE",
                "CI_GATE_NOT_PASS",
                "MAIN_PROTECTION_NOT_EFFECTIVE",
                "CURRENT_MAIN_NOT_HEALTHY",
                "CHECKED_OUT_HEAD_MISMATCH",
                "WORKTREE_NOT_CLEAN",
            },
        )

    def test_merge_command_has_no_admin_or_bypass_path(self) -> None:
        command = build_merge_command(32, HEAD)
        self.assertIn("--match-head-commit", command)
        self.assertIn("--merge", command)
        self.assertNotIn("--admin", command)
        self.assertFalse(any("bypass" in argument for argument in command))

    def test_merge_command_rechecks_remote_snapshot_before_dispatch(self) -> None:
        final_main = "c" * 40
        merged_pr = open_pr()
        merged_pr["state"] = "MERGED"
        merged_pr["mergeCommit"] = {"oid": final_main}
        fake_repository = mock.Mock()
        fake_repository.assert_identity = mock.Mock()
        fake_repository.head.return_value = HEAD
        fake_repository.clean.return_value = True
        fake_client = mock.Mock()
        fake_client.available.return_value = True
        fake_client.pr_view.side_effect = [open_pr(), open_pr(), merged_pr]
        fake_client.protection.side_effect = [
            protection_detail(),
            protection_detail(),
        ]
        fake_client.main_sha.side_effect = [MAIN, MAIN, final_main, final_main]
        fake_client.merge.return_value = CommandResult(0)
        fake_client.commit_parents.return_value = [MAIN, HEAD]
        with tempfile.TemporaryDirectory() as temporary:
            fake_repository.timing_path.side_effect = lambda sha: Path(temporary) / sha
            with (
                mock.patch("tools.dev.train.Repository", return_value=fake_repository),
                mock.patch("tools.dev.train.GithubClient", return_value=fake_client),
                mock.patch(
                    "tools.dev.train.classify_main_commit",
                    return_value=ChangeClass.RUST_OR_RUNTIME_CHANGE,
                ),
                mock.patch("tools.dev.train._main_run_health", return_value="PASS"),
                mock.patch("sys.stdout", io.StringIO()),
            ):
                result = command_merge(
                    argparse.Namespace(
                        pr=32,
                        expected_head=HEAD,
                        expected_base=MAIN,
                    )
                )
        self.assertEqual(result, 0)
        self.assertEqual(fake_client.pr_view.call_count, 3)
        self.assertEqual(fake_client.protection.call_count, 2)
        fake_client.merge.assert_called_once_with(32, HEAD)

    def test_ambiguous_merge_exit_reconciles_to_exact_pr_commit(self) -> None:
        pr = open_pr()
        pr["state"] = "MERGED"
        pr["mergeCommit"] = {"oid": MAIN}
        merge_commit, errors = reconcile_merge_result(
            CommandResult(1), pr, HEAD, MAIN
        )
        self.assertEqual(merge_commit, MAIN)
        self.assertEqual(errors, [])

    def test_merge_reconciliation_refuses_unrelated_main_advance(self) -> None:
        pr = open_pr()
        pr["state"] = "MERGED"
        pr["mergeCommit"] = {"oid": MAIN}
        merge_commit, errors = reconcile_merge_result(
            CommandResult(0), pr, HEAD, "c" * 40
        )
        self.assertEqual(merge_commit, MAIN)
        self.assertIn("AUTHORITATIVE_MAIN_MISMATCH", errors)

    def test_merge_parents_bind_health_checked_main_and_exact_head(self) -> None:
        self.assertEqual(validate_merge_parents([MAIN, HEAD], MAIN, HEAD), [])
        self.assertEqual(
            set(validate_merge_parents(["c" * 40, "d" * 40], MAIN, HEAD)),
            {"UNHEALTHY_BASE_PARENT", "WRONG_HEAD_PARENT"},
        )

    def test_wrong_repository_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Repository(Path(temporary), FakeRunner())
            with self.assertRaises(ToolError):
                repository.assert_identity()

    def test_expected_remote_forms_are_recognized(self) -> None:
        self.assertEqual(
            normalize_repository_identity(
                "https://github.com/jerry-devlab/runnermesh.git"
            ),
            EXPECTED_REPOSITORY,
        )
        self.assertEqual(
            normalize_repository_identity("git@github.com:jerry-devlab/runnermesh.git"),
            EXPECTED_REPOSITORY,
        )

    def test_protection_requires_ci_gate_and_no_bypass(self) -> None:
        detail = protection_detail()
        self.assertTrue(protection_is_effective(detail))
        detail["bypass_actors"] = [{"actor_id": 1}]
        self.assertFalse(protection_is_effective(detail))
        detail["bypass_actors"] = []
        detail["conditions"]["ref_name"]["exclude"] = ["refs/heads/*"]
        self.assertFalse(protection_is_effective(detail))
        detail["conditions"]["ref_name"]["exclude"] = []
        detail["rules"] = [rule for rule in detail["rules"] if rule["type"] != "deletion"]
        self.assertFalse(protection_is_effective(detail))

    def test_protection_rejects_ambiguous_ref_globs(self) -> None:
        detail = {
            "enforcement": "active",
            "bypass_actors": [],
            "conditions": {"ref_name": {"include": ["refs/*"]}},
            "rules": [
                {"type": "deletion"},
                {"type": "pull_request"},
                {"type": "non_fast_forward"},
                {
                    "type": "required_status_checks",
                    "parameters": {
                        "required_status_checks": [{"context": CI_GATE_NAME}]
                    },
                },
            ],
        }
        self.assertFalse(protection_is_effective(detail))

        detail["conditions"]["ref_name"]["include"] = ["refs/heads/*"]
        self.assertFalse(protection_is_effective(detail))

        detail["conditions"]["ref_name"]["include"] = ["refs/heads/main"]
        detail["conditions"]["ref_name"]["exclude"] = ["refs/*"]
        self.assertFalse(protection_is_effective(detail))

    def test_protection_rejects_unresolved_default_branch_token(self) -> None:
        detail = {
            "enforcement": "active",
            "bypass_actors": [],
            "conditions": {"ref_name": {"include": ["~DEFAULT_BRANCH"]}},
            "rules": [
                {"type": "deletion"},
                {"type": "pull_request"},
                {"type": "non_fast_forward"},
                {
                    "type": "required_status_checks",
                    "parameters": {
                        "required_status_checks": [{"context": CI_GATE_NAME}]
                    },
                },
            ],
        }
        self.assertFalse(protection_is_effective(detail))

        detail["conditions"]["ref_name"] = {
            "include": ["refs/heads/main"],
            "exclude": ["~DEFAULT_BRANCH"],
        }
        self.assertFalse(protection_is_effective(detail))

        detail["conditions"]["ref_name"] = {"include": ["~ALL"]}
        self.assertTrue(protection_is_effective(detail))

    def test_non_strict_protection_and_unchanged_exact_base_pass(self) -> None:
        detail = protection_detail(strict=False)
        self.assertTrue(protection_is_effective(detail))
        self.assertTrue(protection_supports_safe_merge(detail))
        self.assertEqual(
            validate_merge_preconditions(
                open_pr(),
                HEAD,
                expected_base=MAIN,
                current_main=MAIN,
                repository_ok=True,
                protection_ok=protection_supports_safe_merge(detail),
                current_main_health="PASS",
                checked_out_head=HEAD,
            ),
            [],
        )

    def test_non_strict_protection_refuses_advanced_main_as_base_stale(self) -> None:
        errors = validate_merge_preconditions(
            open_pr(),
            HEAD,
            expected_base=MAIN,
            current_main="c" * 40,
            repository_ok=True,
            protection_ok=protection_supports_safe_merge(protection_detail()),
            current_main_health="PASS",
            checked_out_head=HEAD,
        )
        self.assertIn("BASE_STALE", errors)

    def test_safe_merge_refuses_exact_head_mismatch(self) -> None:
        errors = validate_merge_preconditions(
            open_pr("c" * 40),
            HEAD,
            expected_base=MAIN,
            current_main=MAIN,
            repository_ok=True,
            protection_ok=True,
            current_main_health="PASS",
            checked_out_head=HEAD,
        )
        self.assertIn("STALE_HEAD", errors)

    def test_safe_merge_refuses_ci_gate_not_pass(self) -> None:
        errors = validate_merge_preconditions(
            open_pr(gate="FAILURE"),
            HEAD,
            expected_base=MAIN,
            current_main=MAIN,
            repository_ok=True,
            protection_ok=True,
            current_main_health="PASS",
            checked_out_head=HEAD,
        )
        self.assertIn("CI_GATE_NOT_PASS", errors)

    def test_safe_merge_refuses_bypass_or_protection_drift(self) -> None:
        detail = protection_detail()
        detail["bypass_actors"] = [{"actor_id": 1}]
        self.assertFalse(protection_supports_safe_merge(detail))
        detail = protection_detail()
        detail["rules"] = [
            rule for rule in detail["rules"] if rule["type"] != "non_fast_forward"
        ]
        self.assertFalse(protection_supports_safe_merge(detail))

    def test_strict_freshness_still_passes_with_all_other_invariants(self) -> None:
        detail = protection_detail(strict=True)
        self.assertTrue(protection_supports_safe_merge(detail))
        self.assertEqual(
            validate_merge_preconditions(
                open_pr(),
                HEAD,
                expected_base=MAIN,
                current_main=MAIN,
                repository_ok=True,
                protection_ok=protection_supports_safe_merge(detail),
                current_main_health="PASS",
                checked_out_head=HEAD,
            ),
            [],
        )

    def test_safe_merge_rejects_merge_queue(self) -> None:
        detail = protection_detail()
        detail["rules"].append({"type": "merge_queue"})
        self.assertFalse(protection_is_effective(detail))
        self.assertFalse(protection_supports_safe_merge(detail))

    def test_next_merge_requires_main_health_and_safe_protection(self) -> None:
        detail = protection_detail(strict=False)
        self.assertTrue(next_merge_allowed("PASS", detail))
        self.assertFalse(next_merge_allowed("FAIL", detail))


class MainHealthTests(unittest.TestCase):
    def test_docs_main_health_passes_only_with_lightweight_jobs(self) -> None:
        run = {"jobs": successful_jobs(ChangeClass.DOCS_ONLY)}
        healthy, problems = validate_main_jobs(run, ChangeClass.DOCS_ONLY)
        self.assertTrue(healthy)
        self.assertEqual(problems, [])

    def test_docs_main_accepts_pre_fast_path_full_cross_platform_evidence(self) -> None:
        run = {"jobs": successful_jobs(ChangeClass.RUST_OR_RUNTIME_CHANGE)}
        healthy, problems = validate_main_jobs(run, ChangeClass.DOCS_ONLY)
        self.assertTrue(healthy)
        self.assertEqual(problems, [])

    def test_code_main_health_requires_both_operating_systems(self) -> None:
        run = {"jobs": successful_jobs(ChangeClass.RUST_OR_RUNTIME_CHANGE)}
        healthy, problems = validate_main_jobs(
            run, ChangeClass.RUST_OR_RUNTIME_CHANGE
        )
        self.assertTrue(healthy)
        self.assertEqual(problems, [])
        run["jobs"] = [
            job
            for job in run["jobs"]
            if job["name"] != "Cargo tests + Clippy (ubuntu-latest)"
        ]
        healthy, problems = validate_main_jobs(
            run, ChangeClass.RUST_OR_RUNTIME_CHANGE
        )
        self.assertFalse(healthy)
        self.assertIn("Cargo tests + Clippy (ubuntu-latest)", problems)

    def test_current_main_health_rejects_incomplete_successful_job_shape(self) -> None:
        jobs = successful_jobs(ChangeClass.RUST_OR_RUNTIME_CHANGE)
        jobs = [
            job
            for job in jobs
            if job["name"] != "Cargo tests + Clippy (ubuntu-latest)"
        ]
        run = {
            "status": "completed",
            "conclusion": "success",
            "jobs": jobs,
        }
        self.assertEqual(
            _main_run_health(
                FakeGithub(run=run), MAIN, ChangeClass.RUST_OR_RUNTIME_CHANGE
            ),
            "FAIL",
        )

    def test_wait_main_reports_terminal_failure(self) -> None:
        run = {
            "status": "completed",
            "conclusion": "failure",
            "startedAt": "2026-09-01T00:00:00Z",
            "updatedAt": "2026-09-01T00:00:10Z",
            "jobs": [check(CI_GATE_NAME, "failure")],
        }
        result = wait_for_main(
            FakeGithub(run=run),
            MAIN,
            ChangeClass.RUST_OR_RUNTIME_CHANGE,
            timeout=30,
            interval=5,
            progress=lambda _: None,
        )
        self.assertEqual(result.status, "FAIL")
        self.assertEqual(result.problems, (CI_GATE_NAME,))

    def test_wait_main_reports_pass(self) -> None:
        run = {
            "status": "completed",
            "conclusion": "success",
            "startedAt": "2026-09-01T00:00:00Z",
            "updatedAt": "2026-09-01T00:00:10Z",
            "jobs": successful_jobs(ChangeClass.RUST_OR_RUNTIME_CHANGE),
        }
        result = wait_for_main(
            FakeGithub(run=run),
            MAIN,
            ChangeClass.RUST_OR_RUNTIME_CHANGE,
            timeout=30,
            interval=5,
            progress=lambda _: None,
        )
        self.assertEqual(result.status, "PASS")

    def test_wait_main_rechecks_authoritative_ref_before_pass(self) -> None:
        run = {
            "status": "completed",
            "conclusion": "success",
            "jobs": successful_jobs(ChangeClass.RUST_OR_RUNTIME_CHANGE),
        }
        result = wait_for_main(
            FakeGithub(run=run, main_shas=[MAIN, "c" * 40]),
            MAIN,
            ChangeClass.RUST_OR_RUNTIME_CHANGE,
            timeout=30,
            interval=5,
            progress=lambda _: None,
        )
        self.assertEqual(result.status, "STALE_MAIN")

    def test_wait_main_rejects_pass_observed_after_deadline(self) -> None:
        run = {
            "status": "completed",
            "conclusion": "success",
            "jobs": successful_jobs(ChangeClass.RUST_OR_RUNTIME_CHANGE),
        }
        clock = MutableClock()
        result = wait_for_main(
            FakeGithub(run=run, clock=clock, call_delay=11),
            MAIN,
            ChangeClass.RUST_OR_RUNTIME_CHANGE,
            timeout=10,
            interval=5,
            clock=clock,
            sleeper=clock.sleep,
            progress=lambda _: None,
        )
        self.assertEqual(result.status, "TIMEOUT")


class TimingAndAvailabilityTests(unittest.TestCase):
    def test_wait_main_command_rechecks_ref_after_protection_lookup(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            timing_path = Path(temporary) / "timing.json"
            fake_repository = mock.Mock()
            fake_repository.assert_identity = mock.Mock()
            fake_repository.fetch_main = mock.Mock()
            fake_repository.timing_path.return_value = timing_path
            fake_client = mock.Mock()
            fake_client.available.return_value = True
            fake_client.main_sha.side_effect = [MAIN, "c" * 40]
            fake_client.protection.return_value = {
                "required_status_checks": {
                    "strict_required_status_checks_policy": True
                },
                "pull_request": True,
                "non_fast_forward": True,
                "bypass_actors": [],
            }
            wait_result = mock.Mock(
                status="PASS",
                ended_at=20.0,
                run_started_at=10.0,
                problems=(),
            )
            output = io.StringIO()
            with (
                mock.patch("tools.dev.train.Repository", return_value=fake_repository),
                mock.patch("tools.dev.train.GithubClient", return_value=fake_client),
                mock.patch("tools.dev.train.resolve_head", return_value=MAIN),
                mock.patch("tools.dev.train.resolve_base", return_value=HEAD),
                mock.patch(
                    "tools.dev.train.git_changed_paths",
                    return_value=["tools/dev/train.py"],
                ),
                mock.patch(
                    "tools.dev.train.classify_paths",
                    return_value=ChangeClass.RUST_OR_RUNTIME_CHANGE,
                ),
                mock.patch("tools.dev.train.wait_for_main", return_value=wait_result),
                mock.patch("sys.stdout", output),
            ):
                result = command_wait_main(
                    argparse.Namespace(expected_main=MAIN, timeout=30, interval=5)
                )
        self.assertEqual(result, 1)
        self.assertIn("MAIN_CI=STALE_MAIN", output.getvalue())
        self.assertIn("NEXT_MERGE_ALLOWED=false", output.getvalue())
        self.assertEqual(fake_client.main_sha.call_count, 2)

    def test_wait_pr_command_refuses_checkout_not_at_expected_head(self) -> None:
        fake_repository = mock.Mock()
        fake_repository.assert_identity = mock.Mock()
        fake_repository.head.return_value = "c" * 40
        output = io.StringIO()
        with (
            mock.patch("tools.dev.train.Repository", return_value=fake_repository),
            mock.patch("tools.dev.train.GithubClient") as github_client,
            mock.patch("sys.stdout", output),
        ):
            result = command_wait_pr(
                argparse.Namespace(
                    expected_head=HEAD,
                    pr=32,
                    timeout=30,
                    interval=5,
                )
            )
        self.assertEqual(result, 1)
        self.assertIn("PR_CI=STALE_HEAD", output.getvalue())
        github_client.assert_not_called()

    def test_candidate_command_initializes_timing_store(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            timing_path = Path(temporary) / "timing.json"
            fake_repository = mock.Mock()
            fake_repository.assert_identity = mock.Mock()
            fake_repository.clean.return_value = True
            fake_repository.head.return_value = HEAD
            fake_repository.timing_path.return_value = timing_path
            with (
                mock.patch("tools.dev.train.Repository", return_value=fake_repository),
                mock.patch("tools.dev.train.resolve_head", return_value=HEAD),
                mock.patch("tools.dev.train.resolve_base", return_value=MAIN),
                mock.patch(
                    "tools.dev.train.git_changed_paths",
                    return_value=["tools/dev/train.py"],
                ),
                mock.patch(
                    "tools.dev.train.subprocess.run",
                    return_value=subprocess.CompletedProcess([], 0),
                ),
                mock.patch("tools.dev.train.run_portability") as portability,
                mock.patch("sys.stdout", new_callable=io.StringIO),
            ):
                portability.return_value.status = "N/A"
                portability.return_value.detail = "test"
                result = command_candidate(
                    argparse.Namespace(base=MAIN, portability="off")
                )
            values = TimingStore(timing_path).load()
        self.assertEqual(result, 0)
        self.assertEqual(values["candidate_head"], HEAD)
        self.assertIn("candidate_start", values)
        self.assertIn("candidate_ready", values)

    def test_candidate_refuses_stale_worktree_after_validation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            timing_path = Path(temporary) / "timing.json"
            fake_repository = mock.Mock()
            fake_repository.assert_identity = mock.Mock()
            fake_repository.clean.side_effect = [True, False]
            fake_repository.head.return_value = HEAD
            fake_repository.timing_path.return_value = timing_path
            output = io.StringIO()
            with (
                mock.patch("tools.dev.train.Repository", return_value=fake_repository),
                mock.patch("tools.dev.train.resolve_head", return_value=HEAD),
                mock.patch("tools.dev.train.resolve_base", return_value=MAIN),
                mock.patch(
                    "tools.dev.train.git_changed_paths",
                    return_value=["tools/dev/train.py"],
                ),
                mock.patch(
                    "tools.dev.train.subprocess.run",
                    return_value=subprocess.CompletedProcess([], 0),
                ),
                mock.patch("tools.dev.train.run_portability") as portability,
                mock.patch("sys.stdout", output),
            ):
                portability.return_value.status = "N/A"
                portability.return_value.detail = "test"
                result = command_candidate(
                    argparse.Namespace(base=MAIN, portability="off")
                )
        self.assertEqual(result, 1)
        self.assertIn("LOCAL_GATE=FAIL", output.getvalue())
        self.assertIn("CANDIDATE_STATE=STALE", output.getvalue())
        self.assertNotIn("LOCAL_GATE=PASS", output.getvalue())

    def test_timing_files_are_keyed_by_exact_pipeline_sha(self) -> None:
        self.assertNotEqual(timing_filename(HEAD), timing_filename(MAIN))
        self.assertIn(HEAD, timing_filename(HEAD))
        with self.assertRaises(ToolError):
            timing_filename("../not-a-sha")

    def test_timing_receipt_calculation(self) -> None:
        fields = calculate_timing_fields(
            {
                "candidate_start": 10,
                "candidate_ready": 20,
                "pr_wait_start": 30,
                "pr_ci_end": 50,
                "merge_time": 55,
                "main_ci_start": 56,
                "main_ci_end": 76,
            }
        )
        self.assertEqual(fields["LOCAL_VALIDATION_SECONDS"], 10)
        self.assertEqual(fields["PR_CI_SECONDS"], 20)
        self.assertEqual(fields["MERGE_WAIT_SECONDS"], 5)
        self.assertEqual(fields["MAIN_CI_SECONDS"], 20)
        self.assertEqual(fields["TOTAL_PIPELINE_SECONDS"], 66)

    def test_stale_pipeline_timing_is_discarded(self) -> None:
        self.assertEqual(
            timing_values_for_head(
                {"candidate_head": "c" * 40, "candidate_start": 10}, HEAD
            ),
            {},
        )
        self.assertEqual(
            timing_values_for_head(
                {"candidate_head": HEAD, "candidate_start": 10}, HEAD
            )["candidate_start"],
            10,
        )

    def test_gh_unavailable_is_detected_without_auth_output(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            client = GithubClient(Path(temporary), FakeRunner())
            self.assertFalse(client.available())

    def test_gh_auth_preflight_receives_caller_deadline(self) -> None:
        runner = mock.Mock()
        runner.which.return_value = "gh.exe"
        runner.run.return_value = CommandResult(0)
        client = GithubClient(Path.cwd(), runner)
        self.assertTrue(client.available(timeout=7.5))
        runner.run.assert_called_once_with(
            ("gh", "auth", "status", "--hostname", "github.com"),
            cwd=Path.cwd(),
            timeout=7.5,
        )


if __name__ == "__main__":
    unittest.main()
