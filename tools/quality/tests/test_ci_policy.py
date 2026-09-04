import unittest
from pathlib import Path

from tools.quality.change_classifier import ChangeClass, result_fields
from tools.quality.ci_policy import decide_ci_jobs


class CiPolicyTests(unittest.TestCase):
    def test_docs_pull_request_skips_code_jobs(self) -> None:
        decision = decide_ci_jobs("pull_request", ChangeClass.DOCS_ONLY)
        self.assertFalse(decision.code_ci_required)

    def test_docs_main_push_skips_code_jobs(self) -> None:
        decision = decide_ci_jobs("push", ChangeClass.DOCS_ONLY)
        self.assertFalse(decision.code_ci_required)

    def test_code_pull_request_runs_both_matrix_operating_systems(self) -> None:
        decision = decide_ci_jobs(
            "pull_request", ChangeClass.RUST_OR_RUNTIME_CHANGE
        )
        self.assertTrue(decision.code_ci_required)
        workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("os: [windows-latest, ubuntu-latest]", workflow)

    def test_code_main_push_runs_both_matrix_operating_systems(self) -> None:
        decision = decide_ci_jobs("push", ChangeClass.RUST_OR_RUNTIME_CHANGE)
        self.assertTrue(decision.code_ci_required)
        workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("needs.classify.outputs.code_ci_required == 'true'", workflow)

    def test_manual_dispatch_always_runs_full_code_jobs(self) -> None:
        decision = decide_ci_jobs("workflow_dispatch", ChangeClass.DOCS_ONLY)
        self.assertTrue(decision.code_ci_required)
        fields = result_fields(
            ["README.md"],
            "a" * 40,
            "b" * 40,
            "direct",
            "workflow_dispatch",
        )
        self.assertEqual(fields["change_class"], "DOCS_ONLY")
        self.assertEqual(fields["code_ci_required"], "true")

    def test_classifier_emits_workflow_decision(self) -> None:
        fields = result_fields(
            ["README.md"], "a" * 40, "b" * 40, "direct", "push"
        )
        self.assertEqual(fields["change_class"], "DOCS_ONLY")
        self.assertEqual(fields["code_ci_required"], "false")

    def test_workflow_keeps_stable_gate_and_enables_fail_fast(self) -> None:
        workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("name: CI Gate", workflow)
        self.assertIn("fail-fast: true", workflow)

    def test_hosted_release_binaries_receive_installed_runtime_smoke(self) -> None:
        ci_workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
        rc_workflow = Path(".github/workflows/rc-candidate.yml").read_text(
            encoding="utf-8"
        )
        for workflow in (ci_workflow, rc_workflow):
            self.assertIn("cargo run --locked --example g15r-rc-smoke", workflow)
            self.assertIn("runnermesh-agent.exe", workflow)
            self.assertIn("runnermesh.exe", workflow)
        self.assertIn("if: matrix.os == 'windows-latest'", ci_workflow)
        self.assertLess(
            rc_workflow.index("Execute the installed RC runtime before packaging"),
            rc_workflow.index("Create and independently read back the immutable package"),
        )
        self.assertIn("runnermesh-package.exe.sha256", rc_workflow)
        self.assertIn("operator_helper_sha256", rc_workflow)
        self.assertIn("OPERATOR_INSTALL=PASS", rc_workflow)
        self.assertIn("OPERATOR_UNINSTALL=PASS", rc_workflow)
        self.assertLess(
            rc_workflow.index("Create and independently read back the immutable package"),
            rc_workflow.index("Execute immutable operator install and owned uninstall smoke"),
        )
        self.assertLess(
            rc_workflow.index("Execute immutable operator install and owned uninstall smoke"),
            rc_workflow.index(
                "Upload only the frozen RC package, operator, and verification metadata"
            ),
        )

    def test_only_pull_request_updates_cancel_in_progress(self) -> None:
        workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn(
            "group: ${{ github.workflow }}-${{ github.event_name }}-${{ github.event.pull_request.number || github.sha }}",
            workflow,
        )
        self.assertNotIn("github.event.pull_request.number || github.ref", workflow)
        self.assertIn(
            "cancel-in-progress: ${{ github.event_name == 'pull_request' }}",
            workflow,
        )
        self.assertNotIn("cancel-in-progress: true", workflow)


if __name__ == "__main__":
    unittest.main()
