# RM-160H R2 Security Review Packet (Independent Review Assist)

Status: **READY_FOR_INDEPENDENT_REVIEW** · `INDEPENDENT_SECURITY_REVIEW=PENDING`
Governance Invariant: Implementer (AGY/Cursor) self-review is strictly `SOURCE_LOCAL_PASS` / `DIAGNOSTIC_ONLY` and NEVER counts as `INDEPENDENT_SECURITY_REVIEW=PASS`. External independent security audit remains pending until an authorized read-only auditor delivers an immutable conclusion.

## 1. Candidate HEADs Under Review

This packet provides exact-scope risk and diff maps for external security auditors across two distinct delivery tracks:

| Track | Target | Exact Bound HEAD | Hosted CI Reference | Scope / Purpose |
|---|---|---|---|---|
| **Track 1: v0.1 C0 Baseline** | `H2 Cutover & Dogfood` | `G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab` | Run 42452639359 (6/6 green) | Immutable single-workstation release candidate (BOM repair, versioned install, autostart) |
| **Track 2: v0.2-A LRM** | `HUX-2 / HUX-3 Gate Readiness` | `PACKAGE_BUILD_COMMIT=d481ec236db6d7b26c2e2764f6932ee2bf2f7f98`<br>`PR_65_CANDIDATE_HEAD=d481ec236db6d7b26c2e2764f6932ee2bf2f7f98` | Predecessor Tip: Run 42829617044 (6/6 green) | Local multi-runner manager, Win32 SCM synthetic gate, FakeGithub admission matrix, read-only resource probe, sandbox package installer |

Crossing tracks is forbidden: `H2` must only cut over on `G15R_RC`, never on PR #65 candidate tip.

### Bound Sandbox Candidate Package Artifacts

External reviewers can verify the immutable sandbox package binaries built from `PACKAGE_BUILD_COMMIT`:

```text
CANDIDATE_ZIP=runnermesh-0.1.0-windows-x64.zip
CANDIDATE_ZIP_SHA256=ff4ded170136cd9a5fd3c8e171c71a10e16d815c9a585c181900b3fdb3b3951f
CANDIDATE_ZIP_BYTES=1164617
CANDIDATE_HELPER=runnermesh-package.exe
CANDIDATE_HELPER_SHA256=2547621d7d40f380589a41994d7c66cf5bfc40ac37972ef42cc38fade49c88ea
CANDIDATE_HELPER_BYTES=1777664
PROVENANCE_VERSION=0.1.0
PROVENANCE_COMMIT=d481ec236db6d7b26c2e2764f6932ee2bf2f7f98
PROVENANCE_CHANNEL=rc
PROVENANCE_TARGET=x86_64-pc-windows-msvc
PACKAGE_VERIFY=PASS
INDEPENDENT_HASH_CHECK=PASS
EXTRACT_TEST=PASS
INSTALL_TEST=PASS
SANDBOX_LIFECYCLE_TESTS=PASS (357/357 cargo tests)
```

## 2. Risk & Architecture Map

### Surface A: Win32 SCM Control & Trusted Authorization Boundary (R4 Scope)
- **Files**: `src/windows_scm_executor.rs`, `src/service_control_contract.rs`, `src/hux2_synthetic.rs`, `src/runner_service_backend.rs`, `tests/trusted_owner_auth_integration.rs`
- **Key Invariants**:
  1. `LiveScmExecutionGate`: Gate is sealed fail-closed. Public boolean construction cannot open the gate.
  2. Synthetic vs. Trusted Grant: `SyntheticScmTestGrant` is synthetic-only; `TrustedOwnerAuthorizationVerifier` remains unissued (`UnissuedOwnerAuthorizationVerifier`) in repository code.
  3. Single-Service & Exact Identity: Wildcards, foreign service names, and blanket elevation are refused.
  4. Non-Destructive Lifecycle: `DrainThenStop` requires selector removal and idle worker before any stop call. Destructive Worker `kill` or `taskkill` is prohibited.
  5. Mandatory Prestate & Readback: Independent status and configuration readback must match plan; failure forces `restore_pass=false`.
  6. Windows Backend Mutation Inert: `WindowsScmBackend::mutation_inert()` has write paths (`start`, `stop`, `change_start_mode`) unconditionally return `Err(ScmApiError::GateClosed)`. A privately minted `ScmMutationArm` is required to open writes, unconstructable in public code.
  7. Trusted Verification & P03 9-Dimension Refusal Matrix: `TokenValidatingOwnerVerifier` and `LiveScmExecutionGate::mint_trusted_live` verified. Public self-signing path is closed (`pub(crate) fn mint_trusted_live`). Verification enforces cryptographic HMAC-SHA256 voucher matching over all 9 P03 dimensions (`service_name`, `image_token`, `execution_identity_token`, `target_host_token`, `candidate_commit`, `transaction_id`, `allow_start`, `allow_stop`, `allow_change_config`), validity windows (`not_before_epoch_seconds` / `expires_at_epoch_seconds`), trusted boundary clock ownership (`TrustedClock`), and shared authorization consumption ledger (`SharedAuthorizationLedger`). Crate-external integration tests in `tests/trusted_owner_auth_integration.rs` and unit tests in `src/windows_scm_executor.rs` prove that all refusal paths fail closed: unissued verifier (`AuthorizationNotIssued`), caller self-selected / forged vouchers (`AccessDenied`), empty material (`AuthorizationNotIssued`), wildcard service names (`NameDrift`), service drift (`NameDrift`), image token mismatch (`IdentityMismatch`), execution identity mismatch (`IdentityMismatch`), target host drift (`TargetHostMismatch`), candidate commit drift (`CandidateCommitMismatch`), transaction ID drift (`TransactionMismatch`), excessive rights (`AccessDenied`), empty nonces (`AccessDenied`), inverted validity windows (`AccessDenied`), expired windows against trusted clock (`AccessDenied`), premature windows against trusted clock (`AccessDenied`), identical nonce replays (`AccessDenied`), swapped-nonce replays across the shared ledger (`AccessDenied`), fresh reconstructed-verifier replays across the shared ledger (`AccessDenied`), and debug representation redacting sensitive material. Even with a `TrustedLive` gate, live Windows adapter write paths remain `GateClosed`. Transition to live issuance requires 3 Owner determinations: (a) root authority secret/key provisioning, (b) transaction window duration (15–120 min) and expiry policy, and (c) in-memory voucher injection distribution channel.

### Surface B: GitHub Admission & Multi-Runner Execution (R6 Scope)
- **Files**: `src/admission.rs`, `tests/fake_github_admission.rs`, `src/hux_sandbox.rs`, `src/agent.rs`, `tests/regular_agent_runner_loop.rs`
- **Key Invariants**:
  1. Reserved Selector: Scoped strictly to `runnermesh-admit` under ADR 0004.
  2. Multi-runner Collision: Same-scope dual managed enrollment is refused (`SameScopeManagedAdmissionCollision`).
  3. Desired vs Achieved State: Label removal does not imply immediate job drain; runner status must reach `DRAINED` / `Stopped` through observation.
  4. Multi-Runner Agent Chain & Isolation: Regular Agent entrypoint verification (`tests/regular_agent_runner_loop.rs`) proves that dispatching `RunnersDisable` for runner-a preserves runner-b desired `Full`, achieves active CI execution without interruption (`plan: Noop`, never stopped), persists across Agent reload, and rejects same-scope collisions fail-closed.
  5. Agent Loop SCM Integration & Multi-Runner Zero Control Action Proof: `AgentCore::observe_decide_reconcile` connects regular multi-runner lifecycle reconciliation directly to narrow service control dispatch. Integration test (`tests/regular_agent_runner_loop.rs`) drives regular Agent startup and polling entrypoints with `MockScmBackend` per-service tracking across 4 sequential phases: Phase 1 proves A receives 0 Stop calls while its selector is present (`selector_present: Some(true)`); Phase 2 proves A receives 0 Stop calls while withdrawal status is unknown (`selector_observation_unknown: true`); Phase 3 proves A receives 0 Stop calls while Worker is busy (`worker_present: true`, `lifecycle: Busy`); Phase 4 proves A receives Stop only after independent readback confirms selector is absent, Worker is idle, and plan allows stop (`PerRunnerPlan::Noop`, `PerRunnerAchieved::Drained`). Throughout all 4 phases, Runner B receives ZERO control actions (`mock_scm.service_actions("actions.runner.b").is_empty()`, `mock_scm.service_stop_calls == 0`, `mock_scm.service_start_calls == 0`).
  6. Scoped Selector ADR 0006 Posture: Remains `DRAFT_NOT_ACCEPTED`. Zero live label mutation occurs without explicit Owner decision.

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

## 4. Reviewer Intake Forms

### Historical Review Record (Predecessor Baseline Diagnostic Evidence)

```text
REVIEWER_CLASS=OWNER_INDEPENDENT_AUDITOR
REVIEW_DATE=2026-11-10
TARGET_TRACK=TRACK_2_HUX2_PR65
BOUND_COMMIT_SHA=ff30112c8527f41f49b64a3a6984b1126a876c05
PR_REFERENCE=PR #65 (Candidate Tip: ff30112c8527f41f49b64a3a6984b1126a876c05, PR HEAD: e32f4f33f1c778700e2826c2d228e4d8fc75ccdf)
HOSTED_CI_RUN=42827699507 (PASS_ALL 6/6 jobs)
SOURCE_SURFACES_REVIEWED=Surface A, Surface B, Surface C, Surface D
FINDINGS_SUMMARY=Initial diagnostic review. Retained as historical evidence.
DISPOSITION=DIAGNOSTIC_HISTORY
SIGNATURE_IDENTIFIER=ISO-8601-AUDITOR-SEC-RM160H-R2-HISTORICAL
```

### Current Candidate Review Intake (Pending External Auditor Evaluation)

```text
REVIEWER_CLASS=OWNER_INDEPENDENT_AUDITOR (Authorized Read-Only External)
REVIEW_DATE=2026-11-11
TARGET_TRACK=TRACK_2_HUX2_PR65
STATUS=PENDING_EXTERNAL_AUDITOR_WINDOW
PACKAGE_BUILD_COMMIT=d481ec236db6d7b26c2e2764f6932ee2bf2f7f98
PR_REFERENCE=PR #65 (feat/160h-gt01-field-baseline)
R4_VERIFICATION=PASS_LOCAL (Self-selected raw token refused, unauthorized root refused, atomic ledger permits exactly 1 of 8 concurrent races, Win32 adapter writes sealed GateClosed)
R6_VERIFICATION=PASS_LOCAL (assemble_installed_core wires multi-runner lifecycle backend; 4-phase sequential Stop dispatch verified for A, 0 actions for B, restart persistence verified, ADR 0004 collision refusal verified)
DISPOSITION=PENDING
```
