# RM-160H R2 Security Review Packet (Independent Review Assist)

Status: **ASSIST_READY** · `INDEPENDENT_SECURITY_REVIEW=PENDING`
Governance Invariant: Implementer (AGY/Cursor) self-review is strictly `SOURCE_LOCAL_PASS` and NEVER counts as `INDEPENDENT_SECURITY_REVIEW=PASS`.

## 1. Candidate HEADs Under Review

This packet provides exact-scope risk and diff maps for external security auditors across two distinct delivery tracks:

| Track | Target | Exact Bound HEAD | Hosted CI Reference | Scope / Purpose |
|---|---|---|---|---|
| **Track 1: v0.1 C0 Baseline** | `H2 Cutover & Dogfood` | `G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab` | Run 42452639359 (6/6 green) | Immutable single-workstation release candidate (BOM repair, versioned install, autostart) |
| **Track 2: v0.2-A LRM** | `HUX-2 / HUX-3 Gate Readiness` | `PACKAGE_BUILD_COMMIT=ea0d9a36000d5c993c9e7c5d5c46450bdd1f87b8`<br>`CODE_ANCHOR=b221eaf72336c5caac676b10405c06e8cdd90b06`<br>`CANDIDATE_DOCS_PR_HEAD=96794280ff6b772fcd211a89db16a43faf498559` | Package Build: Run 42743583126 (6/6 green)<br>Docs PR Tip: Run 42765331894 (6/6 green) | Local multi-runner manager, Win32 SCM synthetic gate, FakeGithub admission matrix, read-only resource probe, sandbox package installer |

Crossing tracks is forbidden: `H2` must only cut over on `G15R_RC`, never on PR #65 candidate tip.

### Bound Sandbox Candidate Package Artifacts

External reviewers can verify the immutable sandbox package binaries built from `PACKAGE_BUILD_COMMIT`:

```text
CANDIDATE_ZIP=runnermesh-0.1.0-windows-x64.zip
CANDIDATE_ZIP_SHA256=d6112ce7157a96f8aae937dccb631a4a489fca8cff6b2982c6e30ade4cb049c8
CANDIDATE_ZIP_BYTES=1157861
CANDIDATE_HELPER=runnermesh-package.exe
CANDIDATE_HELPER_SHA256=3740388f7f37cc31483377e99500c2cc0ec1fa8dc107edcc9756be4000bcb123
CANDIDATE_HELPER_BYTES=1751552
PROVENANCE_VERSION=0.1.0
PROVENANCE_COMMIT=ea0d9a36000d5c993c9e7c5d5c46450bdd1f87b8
PROVENANCE_CHANNEL=rc
PROVENANCE_TARGET=x86_64-pc-windows-msvc
PACKAGE_VERIFY=PASS
INDEPENDENT_HASH_CHECK=PASS
EXTRACT_TEST=PASS
INSTALL_TEST=PASS
SANDBOX_LIFECYCLE_TESTS=PASS (338/338 cargo tests)
```

## 2. Risk & Architecture Map

### Surface A: Win32 SCM Control & Trusted Authorization Boundary (R4 Scope)
- **Files**: `src/windows_scm_executor.rs`, `src/service_control_contract.rs`, `src/hux2_synthetic.rs`, `src/runner_service_backend.rs`
- **Key Invariants**:
  1. `LiveScmExecutionGate`: Gate is sealed fail-closed. Public boolean construction cannot open the gate.
  2. Synthetic vs. Trusted Grant: `SyntheticScmTestGrant` is synthetic-only; `TrustedOwnerAuthorizationVerifier` remains unissued (`UnissuedOwnerAuthorizationVerifier`) in repository code.
  3. Single-Service & Exact Identity: Wildcards, foreign service names, and blanket elevation are refused.
  4. Non-Destructive Lifecycle: `DrainThenStop` requires selector removal and idle worker before any stop call. Destructive Worker `kill` or `taskkill` is prohibited.
  5. Mandatory Prestate & Readback: Independent status and configuration readback must match plan; failure forces `restore_pass=false`.
  6. Windows Backend Mutation Inert: `WindowsScmBackend::mutation_inert()` has write paths (`start`, `stop`, `change_start_mode`) unconditionally return `Err(ScmApiError::GateClosed)`. A privately minted `ScmMutationArm` is required to open writes, unconstructable in public code.

### Surface B: GitHub Admission & Multi-Runner Execution (R6 Scope)
- **Files**: `src/admission.rs`, `tests/fake_github_admission.rs`, `src/hux_sandbox.rs`, `src/agent.rs`
- **Key Invariants**:
  1. Reserved Selector: Scoped strictly to `runnermesh-admit` under ADR 0004.
  2. Multi-runner Collision: Same-scope dual managed enrollment is refused (`same_scope_collision`).
  3. Desired vs Achieved State: Label removal does not imply immediate job drain; runner status must reach `DRAINED` / `Stopped` through observation.
  4. Multi-Runner Agent Chain & Isolation: Disabling runner A preserves runner B desired state, worker, service, selector, and registry (`prepare_two_runner_sandbox` isolated layout verified).
  5. Scoped Selector ADR 0006 Posture: Remains `DRAFT_NOT_ACCEPTED`. Zero live label mutation occurs without explicit Owner decision.

### Surface C: Installation & Autostart Boundaries
- **Files**: `src/installation.rs`, `src/package.rs`, `src/cli.rs`
- **Key Invariants**:
  1. Production Isolation: `SOURCE != BUILD != RELEASE != INSTALLED RUNTIME`.
  2. Worktree Protection: Autostart registrations pointing into git worktrees, debug builds, or development target directories are strictly refused.
  3. CLI Diagnostic Mode: `--diagnostic` is read-only and emits diagnostic logs without mutating runtime state.

### Surface D: Resource Telemetry & EcoQoS Capability Detection
- **Files**: `src/runner_resource.rs`, `src/resource_policy_probe.rs`, `goals/RM-P08-RESOURCE-METHODOLOGY.md`
- **Key Invariants**:
  1. Attribution Dispositions: Only `DirectVerified` and verified `ChildTree` processes are counted. Stale (>5,000ms TTL) or ambiguous samples force `Unknown` (never fabricate 0 bytes or zero savings).
  2. Query-Only Probe: Windows kernel version probed dynamically via `ntdll.dll!RtlGetVersion` without elevation. Zero Job Objects created, zero processes attached, zero affinity mutations (`enforcement_authorized: false`).

## 3. Synthetic vs Live Gate Boundary

```text
[ Synthetic Unit Tests / Matrix ]
  - Mock SCM, Fake GitHub, simulated lag & permissions
  - Verifies state transitions, drift handling, and recovery logic
  - NEVER promotes to LIVE PASS

[ Sealed Owner Execution Boundary ]
  - Requires: External Auditor PASS + Fresh Owner Token + Exact Single Service Target
  - Live Win32 Start/Stop/ChangeConfig inert until trusted token verified
  - Bounded 15-120 minute transaction with mandatory automatic restore
```

## 4. Reviewer Intake Form (For External Auditor)

External security reviewers must complete this intake form without recording secrets or private workstation paths:

```text
REVIEWER_CLASS=<OWNER_INDEPENDENT_AUDITOR | EXTERNAL_SECURITY_ENGINEER>
REVIEW_DATE=<YYYY-MM-DD>
TARGET_TRACK=<TRACK_1_H2_G15R | TRACK_2_HUX2_PR65 | BOTH>
BOUND_COMMIT_SHA=<exact 40-char SHA reviewed>
SOURCE_SURFACES_REVIEWED=<list surfaces, e.g. Surface A, Surface B, etc.>
FINDINGS_SUMMARY=<concise public-safe summary, zero secrets>
DISPOSITION=<PASS | FAIL | BLOCKED>
SIGNATURE_IDENTIFIER=<public-safe token or GPG key ID>
```
