# RM-160H-T05-ADMISSION-MULTIRUNNER checkpoint

```text
PROGRAM_ID=RM-160H-DELIVERY-PROGRAM-001
ACTIVE_TRAIN=RM-160H-T05-ADMISSION-MULTIRUNNER
PRIOR_TRAIN=RM-160H-T04-TRUSTED-SCM
ACTIVE_BUDGET=24h
WORKTREE_LABEL=runnermesh-160h
BRANCH=feat/160h-gt01-field-baseline
IMPLEMENTER=AGY
BASE=PR#63@8cd996acfee23e5f2006e7801139c70d0d67551d
PLANNING_MAIN=d2b37444342d085e650f996e5fb4322f775bbe6f
CODE_ANCHOR=4788d0c80b2da7524f2049747727ca94e1006200
LAST_PASS_ALL=88b5232810a71be765c95d9095642d99f365d95d
REMOTE_TIP=88b5232810a71be765c95d9095642d99f365d95d
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

## Completed in GT-05 leaf 1

1. **P05-01/02 AgentCore Single-Authority Multi-Runner Integration** (`src/agent.rs`):
   - Wired `AgentCore::reconcile_runners` directly to `crate::reconcile_multi_runners`.
   - Extracted global policy authority (`user_mode`, `zen`, `hard_safety`, `effective_probes`) directly from `AgentCore` state, preventing duplicate or divergent policy engines.
   - Tested full precedence hierarchy: HardSafety > Zen > runner `desired_enabled=false` > explicit UserMode (ForceCI) > Auto.
   - Proved that manually disabling a runner outranks `ForceCi`, keeping that specific runner Drained without altering its persisted registry state.
   - Proved that Zen override forces all active runners to drain/offline while strictly preserving their configured `desired_enabled` intents in registry.
2. **P05-03/04 Multi-Instance End-to-End Observe->Decide->Reconcile with Backend Independence** (`src/agent.rs`):
   - Enrolled heterogeneous runners: WindowsService (`runner-svc`) and UserSession (`runner-sess`).
   - Proved A/B independence: disabling runner-svc leaves runner-sess completely unaffected in desired state, achieved state, plans, and registry intent.
   - Proved Worker preservation: busy runner with active worker receives `HoldBusy` plan under withdrawal, preventing destructive worker termination.
3. **P05-05 Registry Store Corruption Fail-Closed** (`src/agent.rs`):
   - Proved that initializing `AgentCore` with a corrupted registry file returns `AgentCoreError::Registry`, preventing silent intent erasure or unsafe re-enrollment.
4. **Validation**:
   - Local: all 331 tests pass (315 lib, 3 package, 13 admission model).
   - Clippy: `cargo clippy --all-targets --all-features -- -D warnings` 100% clean with zero warnings.
   - Code formatting: `rustup run 1.98.1-x86_64-pc-windows-msvc cargo fmt -- --check` clean.
   - Public audit: zero sensitive workstation paths or credentials.

## Completed in GT-05 leaf 2

1. **P05-05/06 Multi-Runner Resilience and Failure Matrix** (`src/runner_reconcile.rs`):
   - **Stale PID / Dead Process Recovery**: Proved that when a runner's fresh observation reports `Stopped` with no worker and selector absent under Drained policy (Work mode), it achieves `Drained` immediately without hang or hallucinating active workers.
   - **Remote Link Loss & Readback Uncertainty**: Proved that when selector observation is unknown or None under Idle (policy desiring Full), the system fails closed (`desired: Drained`, `plan: Refuse`, `achieved: Unknown`), and never falsely asserts Full capacity.
   - **Worker Non-Termination Invariant**: Proved that under withdrawal, an active busy worker is held (`HoldBusy`) with state remaining `Transitional`, never issuing kill commands, and transitions cleanly to `Drained` only upon job completion.
   - **Sibling Isolation During Failure**: Proved that failure or uncertainty on one runner does not poison or alter sibling runner state or registry intent.
2. **Validation**:
   - Local: all 332 tests pass (316 lib, 3 package, 13 admission model).
   - Clippy: `cargo clippy --all-targets --all-features -- -D warnings` 100% clean with zero warnings.
   - Code formatting: `rustup run 1.98.1-x86_64-pc-windows-msvc cargo fmt -- --check` clean.
   - Public audit: zero sensitive workstation paths or credentials.

## Next safe goal

```text
NEXT_SAFE_TRAIN=GT-06: RM-160H-T06-UX-INSTALL-HUX2 (P05/P06 agent, CLI, Tray, install/rollback; P07 qualification prep; HUX2 RC/Owner packet, HUX-3 manual checklist)
BUDGET_USED_GT05≈estimate_5.5h
BUDGET_REMAINING_GT05≈estimate_18.5h
WAITING_FOR_OWNER=true
STOP_BEFORE=H2 cutover; HUX-2 live SCM; labels; H3 publish
RECOVERY_INSTRUCTION=resume worktree_label=runnermesh-160h branch=feat/160h-gt01-field-baseline code_anchor=4788d0c80b2da7524f2049747727ca94e1006200; LF line endings; never touch feat/lrm-09 or overwrite productization worktree
```
