# RM-LONG-TRAIN-LRM-RUNTIME-001 — Master Train checklist

Status: **T4_IN_PROGRESS** · `SOURCE_AND_SANDBOX_ONLY` · `PRODUCTION_MUTATION=false`

```text
TRAIN_ID=RM-LONG-TRAIN-LRM-RUNTIME-001
START_MAIN=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
LRM_BASE=#59@ed68ba7
BRANCH=feat/lrm-runtime-integration
WORKTREE=C:\Dev\src\runnermesh-lrm-runtime
BUDGET_HOURS=8-12
LOCAL_GATE=PASS (cargo test --lib 220; clippy -D warnings)
```

## Stages

| Stage | Goal | Status | Evidence |
|---|---|---|---|
| T0 | DRAINED/OFFLINE matrix + #60 gate hygiene | PASS | ADR0004: DRAINED allows Listening; Offline still needs Stopped; #60 no independent-security waiver |
| T1 | Registry ↔ Agent persistence | PASS | `AgentCore::with_registry_store` + FileRegistryStore; sandbox two-runner restart test |
| T2 | Windows read-only discovery | PASS | `windows_runner_discovery.rs` SCM EnumServicesStatusEx; session homes; Linux EmptyService |
| T3 | Agent/CLI/Tray E2E | PASS | runtime loads registry+discovery; Tray shows intent separately from presentation status |
| T4 | Bounded integration + hosted CI | IN_PROGRESS | local gates PASS; Draft PR + hosted CI pending |

## Hard stops

No real Service/label/credential/autostart/H2/v0.1.0; no AGY; Cursor self-review ≠ independent H2 acceptance.

## Recovery

`goals/RM-CONTINUOUS-TRAIN-CHECKPOINT.md` + this file + worktree `runnermesh-lrm-runtime`.
