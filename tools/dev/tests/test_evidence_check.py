import json
import tempfile
import unittest
import zipfile
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
        self.valid_head = "7107b0df19f89e02574a2ecfa84118cab03d7f07"

        # Create authentic ZIP package artifact containing PACKAGE-MANIFEST.json
        self.archive_path = self.root / "runnermesh-0.2.0-rc.1-windows-x64.zip"
        with zipfile.ZipFile(self.archive_path, "w") as zf:
            manifest_json = json.dumps({
                "schema_version": 1,
                "provenance": {
                    "version": "0.2.0-rc.1",
                    "commit": self.valid_head,
                    "channel": "rc",
                    "target": "x86_64-pc-windows-msvc",
                },
                "files": {},
            })
            zf.writestr("PACKAGE-MANIFEST.json", manifest_json)
        self.archive_bytes = self.archive_path.stat().st_size
        self.archive_sha256 = compute_file_sha256(self.archive_path)

        self.helper_path = self.root / "runnermesh-package.exe"
        self.helper_path.write_bytes(b"dummy helper executable bytes")
        self.helper_bytes = len(self.helper_path.read_bytes())
        self.helper_sha256 = compute_file_sha256(self.helper_path)

        # Create valid audit report referencing candidate_head with BLOCKED status (truthful current state)
        self.report_path = self.root / "report.md"
        self.report_path.write_text(
            f"DISPOSITION=BLOCKED\nCANDIDATE_HEAD={self.valid_head}\n6 findings open\n",
            encoding="utf-8",
        )

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
        pass_report.write_text(
            f"DISPOSITION=PASS\nCANDIDATE_HEAD={self.valid_head}\n0 findings open\n",
            encoding="utf-8",
        )

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

    def test_pure_memory_counterexample_1_truncated_or_mismatched_sha(self):
        """Pure-memory counterexample 1: invalid, 39-character or mismatched SHA rejected without disk access."""
        valid_head = "7107b0df19f89e02574a2ecfa84118cab03d7f07"
        data = {
            "candidate_head": valid_head[:39],  # Truncated 39-char SHA
            "package_artifacts": {
                "archive": "archive.zip",
                "archive_sha256": "0" * 64,
                "archive_bytes": 1000,
                "helper": "helper.exe",
                "helper_sha256": "1" * 64,
                "helper_bytes": 2000,
            },
            "r2_audit": {
                "bound_head": valid_head,
                "report": "report.md",
                "disposition": "BLOCKED",
            },
        }
        res = check_evidence_index(data, base_dir=None)
        self.assertFalse(res.is_pass())
        self.assertTrue(any("must be a full 40-character" in e for e in res.errors))

        # Also test candidate/bound head mismatch
        data["candidate_head"] = valid_head
        data["r2_audit"]["bound_head"] = "f" * 40
        res = check_evidence_index(data, base_dir=None)
        self.assertFalse(res.is_pass())
        self.assertTrue(any("does not match candidate_head" in e for e in res.errors))

    def test_pure_memory_counterexample_2_blocked_report_falsely_claimed_as_pass(self):
        """Pure-memory counterexample 2: report with DISPOSITION=BLOCKED claiming PASS rejected in memory."""
        valid_head = "7107b0df19f89e02574a2ecfa84118cab03d7f07"
        data = {
            "candidate_head": valid_head,
            "r2_audit": {
                "bound_head": valid_head,
                "report": "report.md",
                "raw_report_text": "DISPOSITION=BLOCKED\nFindings 1-6 open\n",
                "disposition": "PASS",
                "findings_count": 0,
            },
        }
        res = check_evidence_index(data, base_dir=None)
        self.assertFalse(res.is_pass())
        self.assertTrue(any("raw report specifies DISPOSITION=BLOCKED" in e for e in res.errors))

    def test_pure_memory_counterexample_3_open_findings_falsely_declaring_prepared(self):
        """Pure-memory counterexample 3: open findings falsely declaring SOURCE_SANDBOX_PREPARED rejected."""
        valid_head = "7107b0df19f89e02574a2ecfa84118cab03d7f07"
        data = {
            "candidate_head": valid_head,
            "r2_audit": {
                "bound_head": valid_head,
                "report": "report.md",
                "disposition": "BLOCKED",
                "findings_count": 6,
                "blocker_latched": True,
            },
            "source_and_sandbox_disposition": "SOURCE_SANDBOX_PREPARED",
        }
        res = check_evidence_index(data, base_dir=None)
        self.assertFalse(res.is_pass())
        self.assertTrue(any("cannot declare SOURCE_SANDBOX_PREPARED" in e for e in res.errors))

    def test_pure_memory_counterexample_4_preflight_substituted_for_audit_report(self):
        """Pure-memory counterexample 4: preflight log substituted for independent audit report rejected."""
        valid_head = "7107b0df19f89e02574a2ecfa84118cab03d7f07"
        data = {
            "candidate_head": valid_head,
            "r2_audit": {
                "bound_head": valid_head,
                "report": "evidence/R2-AUDITOR-PREFLIGHT-7107b0d.json",
                "disposition": "PASS",
                "findings_count": 0,
            },
        }
        res = check_evidence_index(data, base_dir=None)
        self.assertFalse(res.is_pass())
        self.assertTrue(any("preflight log cannot be substituted" in e for e in res.errors))

    def test_pure_memory_positive_truthful_remediation_required(self):
        """Pure-memory positive: truthful index with REMEDIATION_REQUIRED passes mechanical validation."""
        valid_head = "7107b0df19f89e02574a2ecfa84118cab03d7f07"
        data = {
            "candidate_head": valid_head,
            "r2_audit": {
                "bound_head": valid_head,
                "report": "report.md",
                "raw_report_text": f"DISPOSITION=BLOCKED\nCANDIDATE_HEAD={valid_head}\n6 findings open\n",
                "disposition": "BLOCKED",
                "findings_count": 6,
                "blocker_latched": True,
            },
            "source_and_sandbox_disposition": "REMEDIATION_REQUIRED",
        }
        res = check_evidence_index(data, base_dir=None)
        self.assertTrue(res.is_pass(), msg=f"Errors: {res.errors}")
        self.assertEqual(res.disposition, "PASS")
        self.assertEqual(res.product_source_status, "REMEDIATION_REQUIRED")
        self.assertEqual(res.product_security_review, "BLOCKED")
        self.assertTrue(res.blocker_latched)

    def test_reproduction_counterexample_1_old_candidate_differs_from_expected_head(self):
        """Counterexample 1: Old candidate SHA differing from expected head fails closed."""
        index_data = self._base_valid_index()
        # Declare candidate as old head, but specify expected_head as current head
        expected_head = "a7354d888aa135da88b122b8d74dbc8591813f12"
        res = check_evidence_index(index_data, base_dir=self.root, expected_head=expected_head)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any("does not match expected_head" in e for e in res.errors),
            msg=f"Expected expected_head mismatch error, got: {res.errors}",
        )

    def test_reproduction_counterexample_2_zip_missing_on_disk_fails_closed(self):
        """Counterexample 2: Physical ZIP archive missing on disk fails closed."""
        index_data = self._base_valid_index()
        index_data["package_artifacts"]["archive"] = "missing-package-archive.zip"
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any("archive file does not exist: missing-package-archive.zip" in e for e in res.errors),
            msg=f"Expected missing archive error, got: {res.errors}",
        )

    def test_reproduction_counterexample_3_helper_digest_or_size_mismatch_fails_closed(self):
        """Counterexample 3: Helper executable missing, byte size mismatch, or SHA256 digest error fails closed."""
        # 3a: Helper file missing on disk
        index_data = self._base_valid_index()
        index_data["package_artifacts"]["helper"] = "nonexistent-helper.exe"
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any("helper file does not exist: nonexistent-helper.exe" in e for e in res.errors),
            msg=f"Expected missing helper error, got: {res.errors}",
        )

        # 3b: Helper file byte size mismatch
        index_data = self._base_valid_index()
        index_data["package_artifacts"]["helper_bytes"] = self.helper_bytes + 500
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any("helper byte size mismatch" in e for e in res.errors),
            msg=f"Expected helper byte mismatch, got: {res.errors}",
        )

        # 3c: Helper file SHA256 digest mismatch
        index_data = self._base_valid_index()
        index_data["package_artifacts"]["helper_sha256"] = "f" * 64
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any("helper SHA256 mismatch" in e for e in res.errors),
            msg=f"Expected helper SHA mismatch, got: {res.errors}",
        )

    def test_reproduction_counterexample_4_spoofed_package_provenance_and_old_partial_report_fails_closed(self):
        """Counterexample 4: Spoofed provenance inside package archive or old partial report fails closed."""
        # 4a: Archive contains PACKAGE-MANIFEST.json with old commit SHA
        spoofed_archive = self.root / "spoofed-runnermesh.zip"
        old_commit = "fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab"
        with zipfile.ZipFile(spoofed_archive, "w") as zf:
            zf.writestr(
                "PACKAGE-MANIFEST.json",
                json.dumps({
                    "schema_version": 1,
                    "provenance": {
                        "version": "0.2.0-rc.1",
                        "commit": old_commit,
                        "channel": "rc",
                        "target": "x86_64-pc-windows-msvc",
                    },
                }),
            )
        index_data = self._base_valid_index()
        index_data["package_artifacts"]["archive"] = spoofed_archive.name
        index_data["package_artifacts"]["archive_bytes"] = spoofed_archive.stat().st_size
        index_data["package_artifacts"]["archive_sha256"] = compute_file_sha256(spoofed_archive)
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any(f"archive PACKAGE-MANIFEST.json commit {old_commit} does not match candidate_head" in e for e in res.errors),
            msg=f"Expected inner archive provenance commit mismatch, got: {res.errors}",
        )

        # 4b: Old partial report targeting different commit SHA
        old_report = self.root / "old_report.md"
        old_report.write_text(
            f"DISPOSITION=BLOCKED\nCANDIDATE_HEAD={old_commit}\nFindings 1-3 only\n",
            encoding="utf-8",
        )
        index_data = self._base_valid_index()
        index_data["r2_audit"]["report"] = old_report.name
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any("targets commit" in e and "does not reference candidate_head" in e or "targets commit" in e for e in res.errors),
            msg=f"Expected report target commit mismatch, got: {res.errors}",
        )

        # 4c: Diagnostic-only report claiming formal PASS
        diag_report = self.root / "diag_report.md"
        diag_report.write_text(
            f"DISPOSITION=PASS\nCANDIDATE_HEAD={self.valid_head}\nEVIDENCE_SCOPE=DIAGNOSTIC_ONLY\n",
            encoding="utf-8",
        )
        index_data = self._base_valid_index()
        index_data["r2_audit"]["report"] = diag_report.name
        index_data["r2_audit"]["disposition"] = "PASS"
        index_data["r2_audit"]["findings_count"] = 0
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any("diagnostic-only and cannot substantiate formal PASS" in e for e in res.errors),
            msg=f"Expected diagnostic report claiming PASS error, got: {res.errors}",
        )

    def test_rejection_of_non_zip_extension_or_corrupt_zip(self):
        """Checker must refuse non-.zip extension or corrupted/non-zip archive file."""
        # Extension not .zip
        index_data = self._base_valid_index()
        index_data["package_artifacts"]["archive"] = "runnermesh.tar.gz"
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(any("must have a .zip extension" in e for e in res.errors))

        # Physical file is not a valid zip
        fake_zip = self.root / "fake.zip"
        fake_zip.write_bytes(b"not a real zip file content")
        index_data2 = self._base_valid_index()
        index_data2["package_artifacts"]["archive"] = fake_zip.name
        index_data2["package_artifacts"]["archive_bytes"] = fake_zip.stat().st_size
        index_data2["package_artifacts"]["archive_sha256"] = compute_file_sha256(fake_zip)
        res2 = check_evidence_index(index_data2, base_dir=self.root)
        self.assertFalse(res2.is_pass())
        self.assertTrue(any("not a valid zip archive" in e for e in res2.errors))

    def test_rejection_of_missing_provenance_in_index_or_archive(self):
        """Checker must refuse missing provenance in package_artifacts or in archive manifest."""
        # Missing in index
        index_data = self._base_valid_index()
        del index_data["package_artifacts"]["provenance"]
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(any("package_artifacts.provenance is missing" in e for e in res.errors))

        # Missing provenance mapping inside archive manifest
        bad_archive = self.root / "bad_manifest.zip"
        with zipfile.ZipFile(bad_archive, "w") as zf:
            zf.writestr("PACKAGE-MANIFEST.json", json.dumps({"schema_version": 1}))
        index_data2 = self._base_valid_index()
        index_data2["package_artifacts"]["archive"] = bad_archive.name
        index_data2["package_artifacts"]["archive_bytes"] = bad_archive.stat().st_size
        index_data2["package_artifacts"]["archive_sha256"] = compute_file_sha256(bad_archive)
        res2 = check_evidence_index(index_data2, base_dir=self.root)
        self.assertFalse(res2.is_pass())
        self.assertTrue(any("missing provenance" in e for e in res2.errors))

    def test_rejection_of_pending_report_claiming_pass(self):
        """Checker must refuse reports declaring PENDING when r2_audit claims PASS."""
        pending_report = self.root / "pending_report.md"
        pending_report.write_text(
            f"DISPOSITION=PENDING\nCANDIDATE_HEAD={self.valid_head}\nFORMAL_R2_STATUS=PENDING\n",
            encoding="utf-8",
        )
        index_data = self._base_valid_index()
        index_data["r2_audit"]["report"] = pending_report.name
        index_data["r2_audit"]["disposition"] = "PASS"
        index_data["r2_audit"]["findings_count"] = 0
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any("report indicates review is PENDING" in e for e in res.errors),
            msg=f"Expected pending report claiming PASS error, got: {res.errors}",
        )

    def test_rejection_of_same_sha_partial_review_claiming_overall_pass(self):
        """Checker must refuse partial reviews on the same SHA claiming overall PASS."""
        partial_report = self.root / "partial_report.md"
        partial_report.write_text(
            f"DISPOSITION=PASS\nCANDIDATE_HEAD={self.valid_head}\nFindings 1-3 only\nAUDIT_SCOPE=PARTIAL\n",
            encoding="utf-8",
        )
        index_data = self._base_valid_index()
        index_data["r2_audit"]["report"] = partial_report.name
        index_data["r2_audit"]["disposition"] = "PASS"
        index_data["r2_audit"]["findings_count"] = 0
        res = check_evidence_index(index_data, base_dir=self.root)
        self.assertFalse(res.is_pass())
        self.assertTrue(
            any("indicates a partial review" in e for e in res.errors),
            msg=f"Expected partial review claiming overall PASS error, got: {res.errors}",
        )

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
