# RM-160H-T03-IDENTITY-SESSION checkpoint

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T03-IDENTITY-SESSION
PRIOR_TRAIN=RM-160H-T02-V01-CLOSEOUT-PREP
ACTIVE_BUDGET=24h
WORKTREE_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
IMPLEMENTER=AGY
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PLANNING_MAIN=d2b37444342d085e650f996e5fb4322f775bbe6f
CODE_ANCHOR=7bf1f47a6b5699ee7cece7a6100aaa7e2c2d5190
LAST_PASS_ALL=ddf34b483720e1c8ec2d090298ca9398662b6516
REMOTE_TIP=ddf34b483720e1c8ec2d090298ca9398662b6516
PRODUCTION_MUTATION=false
SOURCE_AND_SANDBOX_ONLY=true
CANDIDATE_PR=65
CI_STATUS=PASS_ALL
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
HUX2_LIVE / TRUSTED_SCM=WAITING_FOR_OWNER
G-ADR006=DRAFT_NOT_ACCEPTED
H3=WAITING_FOR_OWNER
INDEPENDENT_SECURITY_REVIEW=PENDING
```

## Completed in GT-03 leaf 1

1. **P02 User Session lifecycle hardening**:
   - `prepare_launch` refuses `BusyWorker` if worker is present or busy (preventing launching listener while active worker executes on work root).
   - `prepare_launch` returns `Noop` if listener is already present (preventing duplicate listener spawn).
   - Added `prepare_multi_launch` and `prepare_multi_graceful_stop` with multi-instance isolation proof.
2. **P02 Multi-instance work root collision safety**:
   - Added `RegistryError::DuplicateWorkRoot` to enforce unique work roots across distinct runner homes during enrollment.
3. **P03 SCM restore change_start_mode readback failure**:
   - Exposed `MockScmBackend::change_calls()`.
   - Proved that if `change_start_mode` succeeds but subsequent readback fails, `restore()` returns `stage=RestoreFailed`, `restore_pass=false`, and `achieved=Unknown`.
4. **Validation**:
   - Local: all 311 tests pass (295 lib, 3 package, 13 admission model).
   - Hosted CI: Run `42497980765` on `2f42098` PASS_ALL; Run `42498262451` on `e2c8896` PASS_ALL.

## Completed in GT-03 leaf 2

1. **P03-05/06 SCM synthetic failure matrix hardening** (`src/hux2_synthetic.rs`):
   - Added `interrupted-stop-readback-fail`: stop accepted, query fails → `Interrupted`, `restore_pass=false`, `achieved=Unknown`.
   - Added `interrupted-start-readback-fail`: start accepted, query fails → `Interrupted`, `restore_pass=false`, `achieved=Unknown`.
   - Added `restore-start-mode-readback-interrupted`: mode change accepted, query fails → `RestoreFailed`, `restore_pass=false`, `achieved=Unknown`.
   - Added `concurrent-agent-state-drift`: concurrent worker start during observe → `WaitIdle`, `stop_calls=0`.
2. **P04-06 Admission barrier & state separation** (`src/runner_reconcile.rs`):
   - Proved selector removal alone is not a job assignment barrier (`worker_present = true` keeps achieved at `Transitional`, plan `HoldBusy`).
   - Achieved `Drained` reached only when worker is absent; `Drained` does not require Listener stopped under Work mode (ADR 0004).
   - Zen/Offline policy strictly requires Listener `Stopped` before achieved `Offline` is asserted.
3. **P05-03/06 Service vs Session multi-backend reconcile & no auto-re-enable** (`src/runner_reconcile.rs`):
   - Proved mixed `WindowsService` and `UserSession` multi-runner reconciliation.
   - Proved disabled runner is never auto-re-enabled across multiple reconcile ticks and worker is never killed destructively.
4. **Validation**:
   - Local: all 313 tests pass (297 lib, 3 package, 13 admission model).
   - Hosted CI: Run `42499969141` on `a1d9be0` PASS_ALL.

## Completed in GT-03 leaf 3

1. **P03-02 SCM execution gate boundaries & unissued verifier hardening** (`src/windows_scm_executor.rs`):
   - `closed_gate_refuses_every_mutation_intent_and_reports_closed`: proves CLOSED gate refuses all mutations without authorizing start, stop, or config.
   - `granular_gate_permissions_and_identity_match_strictly_enforced`: proves granular permissions (start does not authorize stop or config) and strict service/image identity matching.
   - `unissued_verifier_rejects_diverse_request_payloads`: proves `UnissuedOwnerAuthorizationVerifier` refuses empty, populated, or forged requests with `AuthorizationNotIssued`.
2. **P04-02 Scoped admission simulation & ADR 0006 unapproved preservation** (`src/scoped_admission_sim.rs`):
   - `mixed_managed_and_unmanaged_or_missing_selector_classifications`: proves unmanaged runners and valid managed runners remain compatible under v0.1 rules, while missing/unrelated selectors are classified as `RefusedMissingSelector`.
   - `migration_sanitizer_and_adr_preservation`: proves proposed migration names are sanitized, but strictly maintain `requires_owner_gate = true`, `mutates_workflows = true`, `approved = false` under unaccepted ADR 0006.
3. **P05/P06 Registry scope coexistence** (`src/runner_registry.rs`):
   - `distinct_scopes_or_unmanaged_runners_coexist_without_collision`: proves runners in distinct scopes or unmanaged runners coexist cleanly without triggering collision refusal.
4. **Validation**:
   - Local: all 319 tests pass (303 lib, 3 package, 13 admission model).
   - Hosted CI: Run `42500546644` on `ddf34b4` PASS_ALL.

## Next safe goal

```text
NEXT_SAFE_GOAL=GT-03-LEAF-04: Multi-runner CLI/Doctor status JSON rendering / tray presentation consistency OR await Owner gates
BUDGET_USED_GT03≈estimate_3.5h
BUDGET_REMAINING_GT03≈estimate_20.5h
WAITING_FOR_OWNER=true
STOP_BEFORE=H2 cutover; HUX-2; real SCM; labels; H3 publish
RECOVERY_INSTRUCTION=resume worktree_label=runnermesh-160h branch=feat/160h-gt01-field-baseline tip=ddf34b483720e1c8ec2d090298ca9398662b6516; LF line endings; never touch feat/lrm-09 or overwrite productization worktree
```
