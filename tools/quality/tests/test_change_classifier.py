import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.quality.change_classifier import (
    ChangeClass,
    classify_paths,
    git_changed_paths,
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
            "tools/dev/train.py",
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

    def test_runtime_to_docs_rename_inspects_both_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            (root / "src").mkdir()
            source = root / "src" / "runtime.rs"
            source.write_text("pub fn value() {}\n", encoding="utf-8")
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(
                [
                    "git",
                    "-c",
                    "user.name=RunnerMesh tests",
                    "-c",
                    "user.email=tests@example.invalid",
                    "commit",
                    "-qm",
                    "base",
                ],
                cwd=root,
                check=True,
            )
            base = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=root,
                check=True,
                capture_output=True,
                text=True,
                encoding="utf-8",
            ).stdout.strip()
            (root / "docs").mkdir()
            source.rename(root / "docs" / "runtime.md")
            subprocess.run(["git", "add", "-A"], cwd=root, check=True)
            subprocess.run(
                [
                    "git",
                    "-c",
                    "user.name=RunnerMesh tests",
                    "-c",
                    "user.email=tests@example.invalid",
                    "commit",
                    "-qm",
                    "rename",
                ],
                cwd=root,
                check=True,
            )
            head = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=root,
                check=True,
                capture_output=True,
                text=True,
                encoding="utf-8",
            ).stdout.strip()
            paths = git_changed_paths(base, head, "direct", root)
        self.assertEqual(set(paths), {"src/runtime.rs", "docs/runtime.md"})
        self.assertEqual(classify_paths(paths), ChangeClass.RUST_OR_RUNTIME_CHANGE)

    def test_path_hints_do_not_claim_semantic_reuse(self) -> None:
        hints = risk_path_hints(["src/tray.rs", "src/lib.rs"])
        self.assertEqual(hints["TRAY_PRESENTATION_PATH_DIFF"], "CHANGED")
        self.assertEqual(hints["RUNNER_CONTROL_PATH_DIFF"], "EMPTY")


if __name__ == "__main__":
    unittest.main()
