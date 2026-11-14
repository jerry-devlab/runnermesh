# RM-160H-T04-TRUSTED-SCM checkpoint

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T04-TRUSTED-SCM
PRIOR_TRAIN=RM-160H-T03-IDENTITY-SESSION
ACTIVE_BUDGET=24h
WORKTREE_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
IMPLEMENTER=AGY
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PLANNING_MAIN=d2b37444342d085e650f996e5fb4322f775bbe6f
CODE_ANCHOR=57305160c5340ae1d7d90be976ffed912cfe08a7
LAST_PASS_ALL=b4c8d8b2f9011a681c626eb3f92d47d4e3204961
REMOTE_TIP=b4c8d8b2f9011a681c626eb3f92d47d4e3204961
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true
CANDIDATE_PR=65
CI_STATUS=PASS_ALL
SOURCE_CLOSEOUT=PASS
```

## Host truth

```text
HUX0_HUMAN_TRAY_VISUAL=PASS_OWNER_ACCEPTED
HUX1_NONEMPTY_LOCAL_DEV_HOST=EMPTY_OK
HUX1_ZENBOOKDUO=PENDING
```

## Owner gates (held in WAITING, not faked)

```text
HUX1_ZENBOOKDUO=PENDING
H2=WAITING_FOR_OWNER
HUX2_LIVE / TRUSTED_SCM_LIVE=WAITING_FOR_OWNER
G-ADR006=DRAFT_NOT_ACCEPTED
H3=WAITING_FOR_OWNER
INDEPENDENT_SECURITY_REVIEW=PENDING
```

## Completed in GT-04 leaf 1

1. **P03-03 Privileged SCM Helper Boundary** (`src/windows_scm_executor.rs`):
   - Defined `PrivilegedScmAction` (`StartService`, `StopService`, `ChangeStartMode`), `PrivilegedScmRequest`, `PrivilegedScmReport`, and `PrivilegedScmHelper`.
   - Proved strict refusal of wildcard names (`*`), empty names, missing transaction IDs, closed gates, foreign services (divergent image/identity tokens), and BusyWorker on stop.
   - Proved mandatory immediate post-action readback to confirm desired state transition.
2. **P03-05/06 Durable SCM Transaction Journal & Crash Recovery** (`src/windows_scm_executor.rs`):
   - Defined `ScmTransactionJournal` with schema version 1, atomic file write/load/clear with reparse point protection.
   - Implemented `reconcile_interrupted_scm_journal` enforcing that a crashed/resumed transaction NEVER assumes `restore_pass: true` without fresh independent readback confirming exact restored service state and admission presence.
   - Proved fail-closed behavior when readback errors or diverged state cannot be restored under a closed gate.
3. **Validation**:
   - Local: all 326 tests pass (310 lib, 3 package, 13 admission model).
   - Code formatting: `rustup run 1.98.1-x86_64-pc-windows-msvc cargo fmt -- --check` clean.
   - Public audit: zero sensitive paths or machine identities.

## Completed in GT-04 leaf 2

1. **P04-03/04 Scoped Label Contracts & Workflow Routing Safety** (`src/scoped_admission_sim.rs`):
   - Implemented `WorkflowRoutingClassification` (`QualifiedOwnedSelector`, `UnqualifiedLegacyRouting`, `AmbiguousOrMissingRunsOn`) and `WorkflowRoutingAudit`.
   - Proved `audit_workflow_job_routing` classifying generic `self-hosted` labels without the owned selector as `UNQUALIFIED_LEGACY_ROUTING`, demonstrating that selector removal alone cannot guarantee no-new-job drain when legacy routing matches.
   - Defined `ScopedAdmissionAction` (`AddSelector`, `RemoveSelector`), `ScopedAdmissionTransaction`, `ScopedAdmissionReceipt`, and `ScopedAdmissionTransport`.
   - Implemented `plan_scoped_label_add` and `plan_scoped_label_remove` enforcing exact add-one / remove-one, strictly rejecting wildcard (`*`) or empty labels, and requiring explicit Owner Gate for live transactions.
   - Implemented `execute_synthetic_scoped_admission` proving fail-closed readback handling (`readback_pass: false`) on mismatch or timeout, and blocking unissued live transactions.
2. **P04-02/07 Same-Scope Dual Managed Refusal & ADR 0006 Draft Status**:
   - Preserved frozen v0.1 `runnermesh-admit` single-binding compatibility.
   - Verified that same-scope dual managed claims strictly refuse without mutation (`BlockedSameScopeCollision`).
   - Verified that proposed namespaced selectors (`runnermesh-admit-<key>`) remain unapproved and Owner-gated (`approved: false`, `requires_owner_gate: true`).
3. **Validation**:
   - Local: all 329 tests pass (313 lib, 3 package, 13 admission model).
   - Code formatting: `rustup run 1.98.1-x86_64-pc-windows-msvc cargo fmt -- --check` clean.
   - Public audit: zero sensitive workstation paths or credentials.

## Next safe goal

```text
NEXT_SAFE_TRAIN=GT-05: RM-160H-T05-ADMISSION-MULTIRUNNER (P04 selector/routing; P05 policy and partial-failure integration; collision/legacy/Busy/Unknown matrix)
BUDGET_USED_GT04≈estimate_4.5h
BUDGET_REMAINING_GT04≈estimate_19.5h
WAITING_FOR_OWNER=true
STOP_BEFORE=H2 cutover; HUX-2 live SCM; labels; H3 publish
RECOVERY_INSTRUCTION=resume worktree_label=runnermesh-160h branch=feat/160h-gt01-field-baseline code_anchor=57305160c5340ae1d7d90be976ffed912cfe08a7; LF line endings; never touch feat/lrm-09 or overwrite productization worktree
```
