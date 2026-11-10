# RM-160H R2 Security Review Packet (Independent Review Assist)

Status: **ASSIST_READY** · `INDEPENDENT_SECURITY_REVIEW=PENDING`
Governance Invariant: Implementer (AGY/Cursor) self-review is strictly `SOURCE_LOCAL_PASS` and NEVER counts as `INDEPENDENT_SECURITY_REVIEW=PASS`.

## 1. Candidate HEADs Under Review

This packet provides exact-scope risk and diff maps for external security auditors across two distinct delivery tracks:

| Track | Target | Exact Bound HEAD | Hosted CI Reference | Scope / Purpose |
|---|---|---|---|---|
| **Track 1: v0.1 C0 Baseline** | `H2 Cutover & Dogfood` | `G15R_RC=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab` | Run 42452639359 (6/6 green) | Immutable single-workstation release candidate (BOM repair, versioned install, autostart) |
| **Track 2: v0.2-A LRM** | `HUX-2 / HUX-3 Gate Readiness` | `PACKAGE_BUILD_COMMIT=82cf40b85994c668e5a3b29c8ed12f43e3e975d5`<br>`PREDECESSOR_DOCS_PR_HEAD=9757405c93cb6fb7d42cf7f7a79e61dbfffc3dbd` | Predecessor Tip: Run 42788141367 (6/6 green) | Local multi-runner manager, Win32 SCM synthetic gate, FakeGithub admission matrix, read-only resource probe, sandbox package installer |

Crossing tracks is forbidden: `H2` must only cut over on `G15R_RC`, never on PR #65 candidate tip.

### Bound Sandbox Candidate Package Artifacts

External reviewers can verify the immutable sandbox package binaries built from `PACKAGE_BUILD_COMMIT`:

```text
CANDIDATE_ZIP=runnermesh-0.1.0-windows-x64.zip
CANDIDATE_ZIP_SHA256=ef023ec670893c61fce68989e2b25e81b75ebea5ab969e2c2cd2ccf4dcdadb35
CANDIDATE_ZIP_BYTES=1159161
CANDIDATE_HELPER=runnermesh-package.exe
CANDIDATE_HELPER_SHA256=6a99f6e81635a8800536184052c17d0b488d40ef289a021293c1992d15a28856
CANDIDATE_HELPER_BYTES=1753600
PROVENANCE_VERSION=0.1.0
PROVENANCE_COMMIT=82cf40b85994c668e5a3b29c8ed12f43e3e975d5
PROVENANCE_CHANNEL=rc
PROVENANCE_TARGET=x86_64-pc-windows-msvc
PACKAGE_VERIFY=PASS
INDEPENDENT_HASH_CHECK=PASS
EXTRACT_TEST=PASS
INSTALL_TEST=PASS
SANDBOX_LIFECYCLE_TESTS=PASS (343/343 cargo tests)
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
  7. Trusted Verification & Refusal Matrix: `TokenValidatingOwnerVerifier` and `LiveScmExecutionGate::mint_trusted_live` verified. Unit tests prove all refusal paths fail closed: empty material (`AuthorizationNotIssued`), tampered tokens (`AccessDenied`), wildcard service names (`NameDrift`), service drift (`NameDrift`), image token mismatch (`IdentityMismatch`), and excessive rights (`AccessDenied`). Even with a `TrustedLive` gate, live Windows adapter write paths remain `GateClosed`.

### Surface B: GitHub Admission & Multi-Runner Execution (R6 Scope)
- **Files**: `src/admission.rs`, `tests/fake_github_admission.rs`, `src/hux_sandbox.rs`, `src/agent.rs`
- **Key Invariants**:
  1. Reserved Selector: Scoped strictly to `runnermesh-admit` under ADR 0004.
  2. Multi-runner Collision: Same-scope dual managed enrollment is refused (`same_scope_collision`).
  3. Desired vs Achieved State: Label removal does not imply immediate job drain; runner status must reach `DRAINED` / `Stopped` through observation.
  4. Multi-Runner Agent Chain & Isolation: Regular Agent entrypoint verification (`agent_entrypoint_runner_a_disable_preserves_runner_b_intent_and_active_execution`) proves that dispatching `RunnersDisable` for runner-a preserves runner-b desired `Full`, achieves active CI execution without interruption (`plan: Noop`, never stopped), persists across Agent reload, and rejects same-scope collisions fail-closed.
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
