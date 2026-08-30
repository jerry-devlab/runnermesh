import unittest

from tools.quality.change_classifier import (
    ChangeClass,
    classify_paths,
    risk_path_hints,
)


class ChangeClassifierTests(unittest.TestCase):
    def test_markdown_and_governance_text_are_docs_only(self) -> None:
        paths = [
            "README.md",
            "docs/architecture.md",
            "goals/example.txt",
            "dev_governance_files/FAST_LANE.md",
        ]
        self.assertEqual(classify_paths(paths), ChangeClass.DOCS_ONLY)

    def test_source_test_and_manifest_paths_are_runtime(self) -> None:
        for path in ("src/lib.rs", "tests/model.rs", "Cargo.toml", "build.rs"):
            with self.subTest(path=path):
                self.assertEqual(
                    classify_paths(["README.md", path]),
                    ChangeClass.RUST_OR_RUNTIME_CHANGE,
                )

    def test_workflow_resources_and_quality_tools_are_runtime(self) -> None:
        for path in (
            ".github/workflows/ci.yml",
            "resources/runnermesh-agent.rc",
            "tools/quality/public_audit.py",
        ):
            with self.subTest(path=path):
                self.assertEqual(
                    classify_paths([path]), ChangeClass.RUST_OR_RUNTIME_CHANGE
                )

    def test_unknown_file_is_conservatively_runtime(self) -> None:
        self.assertEqual(
            classify_paths(["config/example.json"]),
            ChangeClass.RUST_OR_RUNTIME_CHANGE,
        )

    def test_empty_delta_is_conservatively_runtime(self) -> None:
        self.assertEqual(classify_paths([]), ChangeClass.RUST_OR_RUNTIME_CHANGE)

    def test_path_hints_do_not_claim_semantic_reuse(self) -> None:
        hints = risk_path_hints(["src/tray.rs", "src/lib.rs"])
        self.assertEqual(hints["TRAY_PRESENTATION_PATH_DIFF"], "CHANGED")
        self.assertEqual(hints["RUNNER_CONTROL_PATH_DIFF"], "EMPTY")


if __name__ == "__main__":
    unittest.main()
