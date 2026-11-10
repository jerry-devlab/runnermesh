# RM-160H R2 Security Review Packet (Independent Review Assist)

Status: **ASSIST_READY** · `INDEPENDENT_SECURITY_REVIEW=PENDING`
Governance Invariant: Implementer (AGY/Cursor) self-review is strictly `SOURCE_LOCAL_PASS` and NEVER counts as `INDEPENDENT_SECURITY_REVIEW=PASS`.

## 1. Candidate HEADs Under Review

This packet provides exact-scope risk and diff maps for external security auditors across two distinct delivery tracks:

| Track | Target | Exact Bound HEAD | Hosted CI Reference | Scope / Purpose |
|---|---|---|---|---|
| **Track 1: v0.1 C0 Baseline** | `H2 Cutover & Dogfood` | `G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab` | Run 42452639359 (6/6 green) | Immutable single-workstation release candidate (BOM repair, versioned install, autostart) |
| **Track 2: v0.2-A LRM** | `HUX-2 / HUX-3 Gate Readiness` | `PACKAGE_BUILD_COMMIT=028c7efba9ebd3d3925371b356a187eba428041d`<br>`PREDECESSOR_DOCS_PR_HEAD=a2265f503c984b6f06200ebc51ecea604668cf57` | Predecessor Tip: Run 42808203174 (6/6 green) | Local multi-runner manager, Win32 SCM synthetic gate, FakeGithub admission matrix, read-only resource probe, sandbox package installer |

Crossing tracks is forbidden: `H2` must only cut over on `G15R_RC`, never on PR #65 candidate tip.

### Bound Sandbox Candidate Package Artifacts

External reviewers can verify the immutable sandbox package binaries built from `PACKAGE_BUILD_COMMIT`:

```text
CANDIDATE_ZIP=runnermesh-0.1.0-windows-x64.zip
CANDIDATE_ZIP_SHA256=7ad2455dd2a5a713b8abb34ba75830e54fa1afa1d76763cb2741501130b1355e
CANDIDATE_ZIP_BYTES=1150434
CANDIDATE_HELPER=runnermesh-package.exe
CANDIDATE_HELPER_SHA256=a8b86e6ff8ed912720d6e6cb1c89ffa8d1c9bf44e14863d22d10e005b98d4d54
CANDIDATE_HELPER_BYTES=1778176
PROVENANCE_VERSION=0.1.0
PROVENANCE_COMMIT=028c7efba9ebd3d3925371b356a187eba428041d
PROVENANCE_CHANNEL=rc
PROVENANCE_TARGET=x86_64-pc-windows-msvc
PACKAGE_VERIFY=PASS
INDEPENDENT_HASH_CHECK=PASS
EXTRACT_TEST=PASS
INSTALL_TEST=PASS
SANDBOX_LIFECYCLE_TESTS=PASS (344/344 cargo tests)
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
  7. Trusted Verification & Refusal Matrix: `TokenValidatingOwnerVerifier` and `LiveScmExecutionGate::mint_trusted_live` verified. Public self-signing path is closed (`pub(crate) fn mint_trusted_live`). Verification enforces cryptographic SHA-256 token matching, validity windows (`not_before_epoch_seconds` / `expires_at_epoch_seconds`), trusted boundary clock ownership (`TrustedClock`), and shared authorization consumption ledger (`SharedAuthorizationLedger`). Unit tests in `trusted_owner_verifier_refusal_paths` prove that all refusal paths fail closed: empty material (`AuthorizationNotIssued`), tampered tokens (`AccessDenied`), wildcard service names (`NameDrift`), service drift (`NameDrift`), image token mismatch (`IdentityMismatch`), excessive rights (`AccessDenied`), empty nonces (`AccessDenied`), inverted validity windows (`AccessDenied`), caller-tampered evaluation times (`evaluation_time_epoch_seconds: Some(...)` -> `AccessDenied`), expired windows against trusted clock (`AccessDenied`), premature windows against trusted clock (`AccessDenied`), identical nonce replays (`AccessDenied`), swapped-nonce replays across the shared ledger (`AccessDenied`), fresh reconstructed-verifier replays across the shared ledger (`AccessDenied`), and self-selected unissued tokens (`AccessDenied`). Even with a `TrustedLive` gate, live Windows adapter write paths remain `GateClosed`. Transition to live issuance requires 3 Owner determinations: (a) root authority secret/key provisioning, (b) transaction window duration (15–120 min) and expiry policy, and (c) in-memory voucher injection distribution channel.

### Surface B: GitHub Admission & Multi-Runner Execution (R6 Scope)
- **Files**: `src/admission.rs`, `tests/fake_github_admission.rs`, `src/hux_sandbox.rs`, `src/agent.rs`
- **Key Invariants**:
  1. Reserved Selector: Scoped strictly to `runnermesh-admit` under ADR 0004.
  2. Multi-runner Collision: Same-scope dual managed enrollment is refused (`same_scope_collision`).
  3. Desired vs Achieved State: Label removal does not imply immediate job drain; runner status must reach `DRAINED` / `Stopped` through observation.
  4. Multi-Runner Agent Chain & Isolation: Regular Agent entrypoint verification (`agent_entrypoint_runner_a_disable_preserves_runner_b_intent_and_active_execution`) proves that dispatching `RunnersDisable` for runner-a preserves runner-b desired `Full`, achieves active CI execution without interruption (`plan: Noop`, never stopped), persists across Agent reload, and rejects same-scope collisions fail-closed.
  5. Agent Loop SCM Integration & Multi-Runner Zero Control Action Proof: `AgentCore::reconcile_and_dispatch_runners` connects multi-runner reconciliation directly to narrow service control dispatch. Integration test (`agent_cycle_multi_runner_disable_a_leaves_b_zero_control_actions`) drives regular Agent startup and polling entrypoints with `MockScmBackend` per-service tracking across 4 sequential phases: Phase 1 proves A receives 0 Stop calls while its selector is present (`selector_present: Some(true)`); Phase 2 proves A receives 0 Stop calls while withdrawal status is unknown (`selector_observation_unknown: true`); Phase 3 proves A receives 0 Stop calls while Worker is busy (`worker_present: true`, `lifecycle: Busy`); Phase 4 proves A receives Stop only after independent readback confirms selector is absent, Worker is idle, and plan allows stop (`PerRunnerPlan::Noop`, `PerRunnerAchieved::Drained`). Throughout all 4 phases, Runner B receives ZERO control actions (`mock_scm.service_actions("actions.runner.b").is_empty()`, `mock_scm.service_stop_calls == 0`, `mock_scm.service_start_calls == 0`).
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
