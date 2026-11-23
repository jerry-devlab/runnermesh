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
CODE_ANCHOR=4179596bec1345296e35eff01681ccd761a8907e
LAST_PASS_ALL=9bab37d6e4c45b7ee23c683b54fe3c80eb80fb05
REMOTE_TIP=9bab37d6e4c45b7ee23c683b54fe3c80eb80fb05
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


## Completed in GT-03 leaf 4

1. **P05/P06 CLI multi-runner status/list JSON rendering & prepare flags** (`src/cli.rs`):
   - Added `runners_json_renders_faithful_machine_contract_and_respects_prepare_flags`: proves `runners status --json`, `runners list --json`, `runners enroll --prepare`, and `runners disable --when-idle` output faithful schema-versioned machine contracts without leaking sensitive credentials or host paths.
2. **P05/P06 Tray multi-runner presentation & stable action IDs** (`src/tray.rs`):
   - Added `tray_multi_runner_entries_preserve_stable_ids_and_distinguish_service_and_session`: proves stable toggle menu IDs (`control.runners.toggle.*`) across English and Chinese localizations, cached `RunnersRefresh` updates, and distinct toggle behavior for service vs session instances.
3. **P06 Package doctor drift checks & installation integrity** (`src/package.rs`):
   - Added `package_doctor_reports_fail_on_autostart_drift_and_tampered_active_version`: proves `autostart-ownership` failure on drift, `enable_autostart` drift refusal (`InstallError::AutostartDrift`), `installed-state-validity: Fail` + `active-version-validity: Unknown` on missing active binary, and `package-validity: Fail` on corrupt archive.
4. **Validation**:
   - Local: all 322 tests pass (306 lib, 3 package, 13 admission model).
   - Code formatting: `rustup run 1.98.1-x86_64-pc-windows-msvc cargo fmt -- --check` clean.
   - Public audit: zero sensitive paths or machine identities.
   - Hosted CI: Run `42540734394` on `9bab37d` PASS_ALL.

## Next safe goal

```text
NEXT_SAFE_GOAL=await Owner gates (HUX1_ZENBOOKDUO, H2, HUX2_LIVE, TRUSTED_SCM, G-ADR006, H3) OR next authorized train
BUDGET_USED_GT03≈estimate_5.0h
BUDGET_REMAINING_GT03≈estimate_19.0h
WAITING_FOR_OWNER=true
STOP_BEFORE=H2 cutover; HUX-2; real SCM; labels; H3 publish
RECOVERY_INSTRUCTION=resume worktree_label=runnermesh-160h branch=feat/160h-gt01-field-baseline code_anchor=4179596bec1345296e35eff01681ccd761a8907e; LF line endings; never touch feat/lrm-09 or overwrite productization worktree
```
