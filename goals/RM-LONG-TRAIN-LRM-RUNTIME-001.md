# RM-LONG-TRAIN-LRM-RUNTIME-001 — Master Train checklist

Status: **T4_PASS / SOURCE_CANDIDATE_READY** · `SOURCE_AND_SANDBOX_ONLY` · `PRODUCTION_MUTATION=false`

```text
TRAIN_ID=RM-LONG-TRAIN-LRM-RUNTIME-001
START_MAIN=fdd9fe6b90a4d1f79e6100c0646eb705c5f264ab
FINAL_MAIN=ca1dd96e75a606b80d1978f83e973bd2586d36e4
LRM_BASE=#59@ed68ba7
BRANCH=feat/lrm-runtime-integration
PR=61
HEAD=4a68d7053e4d72809482f44ad32cb056e922b31d
CI_RUN=42455171720 PASS
BUDGET_HOURS=8-12
```

## Stages

| Stage | Goal | Status | Evidence |
|---|---|---|---|
| T0 | DRAINED/OFFLINE matrix + #60 gate hygiene | PASS | ADR0004 DRAINED≠ListenerStopped; #60 merged as ledger only; independent security barrier intact |
| T1 | Registry ↔ Agent persistence | PASS | FileRegistryStore in AgentCore/runtime; sandbox two-runner restart test |
| T2 | Windows read-only discovery | PASS | EnumServicesStatusExW + session homes; Linux EmptyService; incomplete SCM omitted |
| T3 | Agent/CLI/Tray E2E | PASS | runtime registry+discovery; Tray intent≠presentation |
| T4 | Bounded integration + hosted CI | PASS | lib 220 local; Windows+Ubuntu hosted CI PASS on #61 |

## Hard stops honored

No real Service/label/credential/autostart/H2/v0.1.0; no AGY; Cursor self-review ≠ independent H2 acceptance; #61 remains Draft off frozen v0.1 main.

## Recovery

`goals/RM-CONTINUOUS-TRAIN-CHECKPOINT.md` + this file + PR #61.
