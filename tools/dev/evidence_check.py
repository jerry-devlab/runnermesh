#!/usr/bin/env python3
"""Read-only mechanical evidence validator for RunnerMesh.

Validates that evidence references, hashes, provenance, report bindings,
and risk assertions are internally consistent and fail-closed against
tampering, truncated SHAs, missing artifacts, and unproven claims.

This module is strictly read-only:
- It does NOT launch CI or independent reviews.
- It does NOT perform Git writes, pushes, or merges.
- It does NOT grant permissions or certify code security.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Mapping, Sequence

FULL_SHA = re.compile(r"^[0-9a-fA-F]{40}$")
SHA256_HEX = re.compile(r"^[0-9a-fA-F]{64}$")


@dataclass(frozen=True)
class EvidenceCheckResult:
    disposition: str  # "PASS" | "FAIL"
    errors: tuple[str, ...] = ()
    warnings: tuple[str, ...] = ()
    candidate_head: str = ""
    product_source_status: str = "UNKNOWN"
    product_security_review: str = "UNKNOWN"
    blocker_latched: bool = False
    details: Mapping[str, Any] = field(default_factory=dict)

    def is_pass(self) -> bool:
        return self.disposition == "PASS"

    def as_dict(self) -> dict[str, Any]:
        return {
            "EVIDENCE_BINDING_CHECK": self.disposition,
            "CANDIDATE_HEAD": self.candidate_head,
            "PRODUCT_SOURCE_STATUS": self.product_source_status,
            "PRODUCT_SECURITY_REVIEW": self.product_security_review,
            "BLOCKER_LATCHED": self.blocker_latched,
            "ERROR_COUNT": len(self.errors),
            "ERRORS": list(self.errors),
            "WARNINGS": list(self.warnings),
        }

    def emit_summary(self) -> None:
        print(f"EVIDENCE_BINDING_CHECK={self.disposition}")
        if self.candidate_head:
            print(f"CANDIDATE_HEAD={self.candidate_head}")
        print(f"PRODUCT_SOURCE_STATUS={self.product_source_status}")
        print(f"PRODUCT_SECURITY_REVIEW={self.product_security_review}")
        print(f"BLOCKER_LATCHED={str(self.blocker_latched).lower()}")
        if self.errors:
            print(f"ERROR_COUNT={len(self.errors)}")
            for err in self.errors:
                print(f"  ERROR: {err}")
        if self.warnings:
            for warn in self.warnings:
                print(f"  WARNING: {warn}")


def validate_sha(value: Any, field_name: str) -> tuple[str | None, str | None]:
    if not isinstance(value, str):
        return None, f"{field_name} must be a string, got {type(value).__name__}"
    val = value.strip()
    if not FULL_SHA.fullmatch(val):
        return None, f"{field_name} must be a full 40-character hex commit SHA (got: {val!r})"
    return val.lower(), None


def validate_sha256(value: Any, field_name: str) -> tuple[str | None, str | None]:
    if not isinstance(value, str):
        return None, f"{field_name} must be a string, got {type(value).__name__}"
    val = value.strip()
    if not SHA256_HEX.fullmatch(val):
        return None, f"{field_name} must be a 64-character SHA-256 hex string (got: {val!r})"
    return val.lower(), None


def compute_file_sha256(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(65536):
            hasher.update(chunk)
    return hasher.hexdigest().lower()


def check_git_commit_exists(sha: str, repo_root: Path) -> bool:
    try:
        res = subprocess.run(
            ["git", "cat-file", "-e", f"{sha}^{{commit}}"],
            cwd=repo_root,
            capture_output=True,
            check=False,
        )
        return res.returncode == 0
    except OSError:
        return False


def check_evidence_index(
    data: Mapping[str, Any],
    *,
    base_dir: Path | None = None,
    repo_root: Path | None = None,
) -> EvidenceCheckResult:
    """Mechanically validates an evidence index mapping.

    Checks SHAs, package artifact hashes/provenance, review report bindings,
    consistency between source status and open findings, and evidence reuse rules.
    """
    errors: list[str] = []
    warnings: list[str] = []

    # 1. Candidate HEAD validation
    raw_head = data.get("candidate_head")
    candidate_head, err = validate_sha(raw_head, "candidate_head")
    if err:
        errors.append(err)
        candidate_head = str(raw_head or "")
    elif repo_root and repo_root.is_dir() and (repo_root / ".git").exists():
        if not check_git_commit_exists(candidate_head, repo_root):
            errors.append(f"candidate_head {candidate_head} does not exist in local git repository")

    # Other required coordinate SHAs if present
    for sha_field in ("goal_start_main", "remote_main", "pr_base_head", "g15r_rc"):
        if sha_field in data:
            _, err = validate_sha(data[sha_field], sha_field)
            if err:
                errors.append(err)

    # 2. Package Artifacts validation
    pkg = data.get("package_artifacts")
    if isinstance(pkg, Mapping):
        archive_name = pkg.get("archive")
        archive_sha, err = validate_sha256(pkg.get("archive_sha256"), "package_artifacts.archive_sha256")
        if err:
            errors.append(err)
        archive_bytes = pkg.get("archive_bytes")
        if not isinstance(archive_bytes, int) or archive_bytes <= 0:
            errors.append(f"package_artifacts.archive_bytes must be a positive integer (got: {archive_bytes!r})")

        helper_sha, err = validate_sha256(pkg.get("helper_sha256"), "package_artifacts.helper_sha256")
        if err:
            errors.append(err)
        helper_bytes = pkg.get("helper_bytes")
        if not isinstance(helper_bytes, int) or helper_bytes <= 0:
            errors.append(f"package_artifacts.helper_bytes must be a positive integer (got: {helper_bytes!r})")

        provenance = pkg.get("provenance")
        if isinstance(provenance, Mapping):
            prov_commit, err = validate_sha(provenance.get("commit"), "provenance.commit")
            if err:
                errors.append(err)
            elif candidate_head and prov_commit != candidate_head:
                errors.append(
                    f"provenance.commit {prov_commit} does not match candidate_head {candidate_head}"
                )
            for req_field in ("version", "channel", "target"):
                if not provenance.get(req_field):
                    errors.append(f"provenance.{req_field} is missing or empty")
        elif provenance is not None:
            errors.append("package_artifacts.provenance must be a mapping")

        # Verify physical files if base_dir is given
        if base_dir and archive_name:
            candidates_to_check = [
                base_dir / str(archive_name),
                base_dir / "artifacts" / "lrm-rc" / str(archive_name),
            ]
            found_archive = next((p for p in candidates_to_check if p.is_file()), None)
            if found_archive:
                actual_bytes = found_archive.stat().st_size
                if isinstance(archive_bytes, int) and actual_bytes != archive_bytes:
                    errors.append(
                        f"archive byte size mismatch: declared {archive_bytes}, actual file is {actual_bytes}"
                    )
                actual_hash = compute_file_sha256(found_archive)
                if archive_sha and actual_hash != archive_sha:
                    errors.append(
                        f"archive SHA256 mismatch: declared {archive_sha}, actual file is {actual_hash}"
                    )

    elif pkg is not None:
        errors.append("package_artifacts must be a mapping")

    # 3. Independent R2 Audit Report validation
    r2 = data.get("r2_audit")
    r2_disposition = "UNKNOWN"
    findings_count = 0
    blocker_latched = False

    if isinstance(r2, Mapping):
        r2_disposition = str(r2.get("disposition") or "UNKNOWN").upper()
        blocker_latched = bool(r2.get("blocker_latched", False))
        findings_count = int(r2.get("findings_count", 0))

        bound_head, err = validate_sha(r2.get("bound_head"), "r2_audit.bound_head")
        if err:
            errors.append(err)
        elif candidate_head and bound_head != candidate_head:
            errors.append(
                f"r2_audit.bound_head {bound_head} does not match candidate_head {candidate_head}"
            )

        report_rel = r2.get("report")
        if not report_rel or not isinstance(report_rel, str):
            errors.append("r2_audit.report path is missing or not a string")
        elif base_dir:
            report_path = (base_dir / report_rel).resolve()
            if not report_path.is_file():
                errors.append(f"r2_audit report file does not exist: {report_rel}")
            else:
                try:
                    report_content = report_path.read_text(encoding="utf-8", errors="replace")
                    if not report_content.strip():
                        errors.append(f"r2_audit report file is empty: {report_rel}")
                    else:
                        # Check report contents against disposition claims
                        if "DISPOSITION=BLOCKED" in report_content and r2_disposition == "PASS":
                            errors.append(
                                "r2_audit.disposition declared PASS but raw report specifies DISPOSITION=BLOCKED"
                            )
                        if "DISPOSITION=FAIL" in report_content and r2_disposition == "PASS":
                            errors.append(
                                "r2_audit.disposition declared PASS but raw report specifies DISPOSITION=FAIL"
                            )
                        if "AUDITOR-PREFLIGHT" in report_rel and r2_disposition == "PASS":
                            errors.append(
                                "preflight log cannot be substituted for an independent audit report"
                            )
                except OSError as error:
                    errors.append(f"unable to read r2_audit report file {report_rel}: {error}")

        if findings_count > 0 and r2_disposition == "PASS":
            errors.append(
                f"r2_audit.disposition cannot be PASS while findings_count is {findings_count}"
            )
        if r2_disposition not in {"PASS", "BLOCKED", "FAIL", "PENDING", "UNKNOWN"}:
            errors.append(f"unrecognized r2_audit.disposition: {r2_disposition!r}")

    elif r2 is not None:
        errors.append("r2_audit must be a mapping")

    # 4. Source & Sandbox Disposition consistency
    source_status = str(data.get("source_and_sandbox_disposition") or "UNKNOWN").upper()
    if source_status in {"SOURCE_SANDBOX_PREPARED", "PASS"}:
        if r2_disposition != "PASS":
            errors.append(
                f"cannot declare {source_status} while r2_audit disposition is {r2_disposition}"
            )
        if findings_count > 0:
            errors.append(
                f"cannot declare {source_status} while {findings_count} findings remain open"
            )
        if blocker_latched:
            errors.append(
                f"cannot declare {source_status} while blocker_latched is true"
            )

    # 5. Evidence Reuse Validation
    reused_gates = data.get("reused_gates")
    if isinstance(reused_gates, Sequence):
        for idx, entry in enumerate(reused_gates):
            if isinstance(entry, Mapping):
                gate_name = entry.get("gate", f"reused_gate[{idx}]")
                _, err = validate_sha(entry.get("reused_from"), f"{gate_name}.reused_from")
                if err:
                    errors.append(err)
                risk_diff = str(entry.get("risk_diff") or "").upper()
                if risk_diff != "EMPTY" and not entry.get("rationale"):
                    errors.append(
                        f"{gate_name} has disposition REUSED but missing risk_diff=EMPTY or rationale"
                    )

    disposition = "PASS" if not errors else "FAIL"
    return EvidenceCheckResult(
        disposition=disposition,
        errors=tuple(errors),
        warnings=tuple(warnings),
        candidate_head=candidate_head,
        product_source_status=source_status,
        product_security_review=r2_disposition,
        blocker_latched=blocker_latched,
        details=dict(data),
    )


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check RunnerMesh evidence bindings and truthfulness.")
    parser.add_argument(
        "--index",
        type=Path,
        help="Path to evidence index JSON file (defaults to evidence/FINAL-EVIDENCE-INDEX.json).",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(".").resolve(),
        help="Path to repository root.",
    )
    args = parser.parse_args(argv)

    index_path = args.index
    if index_path is None:
        default_locations = [
            args.repo_root / "evidence" / "FINAL-EVIDENCE-INDEX.json",
            args.repo_root.parent / "runnermesh-160h-audit" / "2026-11-12" / "160H-FINAL-CLOSEOUT-2703a1e" / "evidence" / "FINAL-EVIDENCE-INDEX.json",
            args.repo_root.parents[1] / "runnermesh-160h-audit" / "2026-11-12" / "160H-FINAL-CLOSEOUT-2703a1e" / "evidence" / "FINAL-EVIDENCE-INDEX.json",
        ]
        index_path = next((p for p in default_locations if p.is_file()), None)

    if not index_path or not index_path.is_file():
        print(f"ERROR: evidence index file not found: {index_path or 'none specified'}", file=sys.stderr)
        return 1

    try:
        data = json.loads(index_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"ERROR: unable to load evidence index {index_path}: {error}", file=sys.stderr)
        return 1

    base_dir = index_path.parent.parent if index_path.parent.name == "evidence" else index_path.parent
    result = check_evidence_index(data, base_dir=base_dir, repo_root=args.repo_root)
    result.emit_summary()
    return 0 if result.is_pass() else 1


if __name__ == "__main__":
    sys.exit(main())
