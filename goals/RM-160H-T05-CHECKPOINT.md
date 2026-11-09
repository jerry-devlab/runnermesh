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
CODE_ANCHOR=3020b0a62281182ccb375d0acc4dca5167027c33
LAST_PASS_ALL=12c2557a310af78cce80f920b5d50c23a231c7da
REMOTE_TIP=12c2557a310af78cce80f920b5d50c23a231c7da
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

## Next safe goal

```text
NEXT_SAFE_GOAL=GT-05-LEAF-02: P05-05/06 Matrix tests for reboot, Agent crash, resume, stale PID observation, selector readback uncertainty, and partial batch failure
BUDGET_USED_GT05≈estimate_3.0h
BUDGET_REMAINING_GT05≈estimate_21.0h
WAITING_FOR_OWNER=true
STOP_BEFORE=H2 cutover; HUX-2 live SCM; labels; H3 publish
RECOVERY_INSTRUCTION=resume worktree_label=runnermesh-160h branch=feat/160h-gt01-field-baseline code_anchor=3020b0a62281182ccb375d0acc4dca5167027c33; LF line endings; never touch feat/lrm-09 or overwrite productization worktree
```
