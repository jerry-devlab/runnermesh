import json
import tempfile
import unittest
from pathlib import Path

from tools.dev.evidence_check import (
    check_evidence_index,
    compute_file_sha256,
    validate_sha,
    validate_sha256,
)


class TestEvidenceCheck(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)

        # Create dummy package artifacts
        self.archive_path = self.root / "runnermesh-0.2.0-rc.1-windows-x64.zip"
        self.archive_path.write_bytes(b"dummy archive contents for testing 12345")
        self.archive_bytes = len(self.archive_path.read_bytes())
        self.archive_sha256 = compute_file_sha256(self.archive_path)

        self.helper_path = self.root / "runnermesh-package.exe"
        self.helper_path.write_bytes(b"dummy helper executable bytes")
        self.helper_bytes = len(self.helper_path.read_bytes())
        self.helper_sha256 = compute_file_sha256(self.helper_path)

        self.valid_head = "7107b0df19f89e02574a2ecfa84118cab03d7f07"

        # Create valid audit report with BLOCKED status (truthful current state)
        self.report_path = self.root / "report.md"
        self.report_path.write_text("DISPOSITION=BLOCKED\n6 findings open\n", encoding="utf-8")

    def tearDown(self):
        self.temp_dir.cleanup()

    def _base_valid_index(self) -> dict:
        return {
            "program_id": "RM-160H-DELIVERY-PROGRAM-001",
            "candidate_head": self.valid_head,
            "goal_start_main": "fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab",
            "remote_main": "d2b37444342d085e650f996e5fb4322f775bbe6f",
            "pr_base_head": "8cd996acfee23e5f2006e7801139c70d0d67551d",
            "package_artifacts": {
                "archive": self.archive_path.name,
                "archive_sha256": self.archive_sha256,
                "archive_bytes": self.archive_bytes,
                "helper": self.helper_path.name,
                "helper_sha256": self.helper_sha256,
                "helper_bytes": self.helper_bytes,
                "provenance": {
                    "channel": "rc",
                    "commit": self.valid_head,
                    "target": "x86_64-pc-windows-msvc",
                    "version": "0.2.0-rc.1",
                },
            },
            "r2_audit": {
                "auditor": "codex (jerry-auditor)",
                "bound_head": self.valid_head,
                "report": "report.md",
                "disposition": "BLOCKED",
                "findings_count": 6,
                "blocker_latched": True,
            },
            "source_and_sandbox_disposition": "REMEDIATION_REQUIRED",
        }

    def test_positive_truthful_index_passes_mechanical_check(self):
        index_data = self._base_valid_index()
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertTrue(result.is_pass(), msg=f"Errors: {result.errors}")
        self.assertEqual(result.disposition, "PASS")
        self.assertEqual(result.candidate_head, self.valid_head)
        self.assertEqual(result.product_source_status, "REMEDIATION_REQUIRED")
        self.assertEqual(result.product_security_review, "BLOCKED")
        self.assertTrue(result.blocker_latched)

    def test_positive_fully_cleared_prepared_index(self):
        # Create passing report
        pass_report = self.root / "pass_report.md"
        pass_report.write_text("DISPOSITION=PASS\n0 findings open\n", encoding="utf-8")

        index_data = self._base_valid_index()
        index_data["r2_audit"]["report"] = "pass_report.md"
        index_data["r2_audit"]["disposition"] = "PASS"
        index_data["r2_audit"]["findings_count"] = 0
        index_data["r2_audit"]["blocker_latched"] = False
        index_data["source_and_sandbox_disposition"] = "SOURCE_SANDBOX_PREPARED"

        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertTrue(result.is_pass(), msg=f"Errors: {result.errors}")
        self.assertEqual(result.product_source_status, "SOURCE_SANDBOX_PREPARED")
        self.assertEqual(result.product_security_review, "PASS")
        self.assertFalse(result.blocker_latched)

    def test_rejects_39_char_sha(self):
        index_data = self._base_valid_index()
        index_data["candidate_head"] = self.valid_head[:39]
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("must be a full 40-character" in e for e in result.errors))

    def test_rejects_non_hex_sha(self):
        index_data = self._base_valid_index()
        index_data["candidate_head"] = "g" * 40
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("must be a full 40-character" in e for e in result.errors))

    def test_rejects_candidate_head_bound_head_mismatch(self):
        index_data = self._base_valid_index()
        index_data["r2_audit"]["bound_head"] = "0" * 40
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("does not match candidate_head" in e for e in result.errors))

    def test_rejects_provenance_commit_mismatch(self):
        index_data = self._base_valid_index()
        index_data["package_artifacts"]["provenance"]["commit"] = "0" * 40
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("provenance.commit" in e and "does not match candidate_head" in e for e in result.errors))

    def test_rejects_declaring_prepared_with_open_findings(self):
        index_data = self._base_valid_index()
        # Attempt to claim SOURCE_SANDBOX_PREPARED while findings remain open (findings_count = 6)
        index_data["source_and_sandbox_disposition"] = "SOURCE_SANDBOX_PREPARED"
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("cannot declare SOURCE_SANDBOX_PREPARED" in e for e in result.errors))

    def test_rejects_audit_pass_claim_when_report_says_blocked(self):
        index_data = self._base_valid_index()
        index_data["r2_audit"]["disposition"] = "PASS"
        index_data["r2_audit"]["findings_count"] = 0
        # report.md contains DISPOSITION=BLOCKED
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("raw report specifies DISPOSITION=BLOCKED" in e for e in result.errors))

    def test_rejects_missing_package_file(self):
        index_data = self._base_valid_index()
        index_data["package_artifacts"]["archive"] = "nonexistent.zip"
        # We also test when declared file is checked
        index_data["package_artifacts"]["archive_bytes"] = -1
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("archive_bytes must be a positive integer" in e for e in result.errors))

    def test_rejects_package_byte_or_hash_mismatch(self):
        index_data = self._base_valid_index()
        index_data["package_artifacts"]["archive_bytes"] = self.archive_bytes + 999
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("archive byte size mismatch" in e for e in result.errors))

        index_data = self._base_valid_index()
        index_data["package_artifacts"]["archive_sha256"] = "0" * 64
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("archive SHA256 mismatch" in e for e in result.errors))

    def test_rejects_preflight_substituted_for_audit(self):
        index_data = self._base_valid_index()
        index_data["r2_audit"]["report"] = "evidence/R2-AUDITOR-PREFLIGHT-foo.json"
        index_data["r2_audit"]["disposition"] = "PASS"
        (self.root / "evidence").mkdir(exist_ok=True)
        (self.root / "evidence" / "R2-AUDITOR-PREFLIGHT-foo.json").write_text("{}", encoding="utf-8")
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("preflight log cannot be substituted" in e for e in result.errors))

    def test_rejects_reused_without_rationale(self):
        index_data = self._base_valid_index()
        index_data["reused_gates"] = [
            {"gate": "TRAY_GATE", "reused_from": self.valid_head, "risk_diff": "CHANGED"}
        ]
        result = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(result.is_pass())
        self.assertTrue(any("missing risk_diff=EMPTY or rationale" in e for e in result.errors))

    def test_validate_sha_helpers(self):
        sha, err = validate_sha("a" * 40, "test")
        self.assertIsNone(err)
        self.assertEqual(sha, "a" * 40)

        sha, err = validate_sha("a" * 39, "test")
        self.assertIsNotNone(err)

        sha, err = validate_sha(123, "test")
        self.assertIsNotNone(err)

        sha, err = validate_sha256("b" * 64, "test")
        self.assertIsNone(err)
        self.assertEqual(sha, "b" * 64)

        sha, err = validate_sha256("b" * 63, "test")
        self.assertIsNotNone(err)


if __name__ == "__main__":
    unittest.main()
